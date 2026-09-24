// Tinymist LSP proxy - subprocess communication
//
// This module manages a tinymist subprocess and forwards LSP requests/responses.
// It handles:
// - Spawning and managing the tinymist process
// - Sending LSP requests via JSON-RPC
// - Receiving LSP responses and notifications asynchronously
// - Graceful shutdown

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder, FramedRead};
use tower_lsp::lsp_types::Url;

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

/// A codec for LSP messages (Content-Length + JSON-RPC)
pub struct LspCodec;

impl Decoder for LspCodec {
    type Item = Value;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        let src_buf = &src[..];

        // 1. Find the double CRLF that separates headers from body
        let header_end = src_buf.windows(4).position(|w| w == b"\r\n\r\n");

        if let Some(end_pos) = header_end {
            // 2. Parse Content-Length from headers
            let headers = std::str::from_utf8(&src_buf[..end_pos])?;
            let mut content_length = None;
            for line in headers.lines() {
                if line.to_lowercase().starts_with("content-length:")
                    && let Some(len_str) = line.split(':').nth(1)
                {
                    content_length = Some(len_str.trim().parse::<usize>()?);
                }
            }

            if let Some(len) = content_length {
                let total_len = end_pos + 4 + len;
                if src.len() >= total_len {
                    // 3. We have the full message
                    src.advance(end_pos + 4);
                    let data = src.split_to(len);
                    let message: Value = serde_json::from_slice(&data)?;
                    return Ok(Some(message));
                }
            }
        }

        // Not enough data yet
        Ok(None)
    }
}

impl Encoder<Value> for LspCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: Value, dst: &mut BytesMut) -> Result<()> {
        let content = serde_json::to_string(&item)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        dst.reserve(header.len() + content.len());
        dst.put(header.as_bytes());
        dst.put(content.as_bytes());

        Ok(())
    }
}

/// A proxy to a tinymist LSP subprocess
#[derive(Clone)]
pub struct TinymistProxy {
    /// Writer to send data to tinymist's stdin
    stdin: Arc<Mutex<ChildStdin>>,
    /// Handle to the child process (kept for shutdown)
    child: Arc<Mutex<Option<Child>>>,
    /// Counter for request IDs
    request_id: Arc<AtomicU64>,
    /// Map of pending requests (ID -> Sender)
    pending_requests: PendingRequests,
}

// ============================================================================
// Subprocess Management
// ============================================================================

impl TinymistProxy {
    /// Spawn a new tinymist subprocess
    pub async fn spawn(
        root_uri: Option<Url>,
        path_override: Option<PathBuf>,
    ) -> Result<(Self, mpsc::Receiver<Value>)> {
        let config = root_uri
            .as_ref()
            .and_then(|uri| uri.to_file_path().ok())
            .map(|root| knot_core::Config::find_and_load(&root).map(|(config, _)| config))
            .transpose()?
            .unwrap_or_default();
        let tinymist_path = knot_core::tools::resolve_binary(
            "tinymist",
            config.tools.tinymist.as_deref(),
            path_override.as_deref(),
        )?;

        let mut child = Command::new(&tinymist_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn tinymist process")?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;

        let mut framed_read = FramedRead::new(stdout, LspCodec);
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, notification_rx) = mpsc::channel(100);

        // Spawn background task to read from tinymist
        let pending_requests_clone = pending_requests.clone();
        tokio::spawn(async move {
            use futures::StreamExt;
            while let Some(res) = framed_read.next().await {
                match res {
                    Ok(message) => {
                        if let Ok(json) = serde_json::to_string(&message) {
                            log::info!("LSP IN: {}", json);
                        }
                        Self::dispatch_message(message, &pending_requests_clone, &notification_tx)
                            .await;
                    }
                    Err(e) => {
                        eprintln!("Tinymist read error: {}", e);
                        break;
                    }
                }
            }
            // Release outstanding callers immediately when the stream closes.
            pending_requests_clone.lock().await.clear();
        });

        let proxy = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            child: Arc::new(Mutex::new(Some(child))),
            request_id: Arc::new(AtomicU64::new(1)),
            pending_requests,
        };

        if let Err(error) = proxy.initialize(root_uri).await {
            if let Err(cleanup) = proxy.shutdown().await {
                log::warn!("Failed to clean up Tinymist after initialization failure: {cleanup:#}");
            }
            return Err(error);
        }

        Ok((proxy, notification_rx))
    }

    async fn dispatch_message(
        message: Value,
        pending_requests: &PendingRequests,
        notification_tx: &mpsc::Sender<Value>,
    ) {
        // Server-initiated request or notification
        if message.get("method").is_some() {
            if let Err(e) = notification_tx.try_send(message) {
                log::warn!("Dropped Tinymist notification: {}", e);
            }
            return;
        }

        // Response to one of our requests: has "id", no "method".
        if let Some(id_val) = message.get("id").and_then(|id| id.as_u64()) {
            let mut map = pending_requests.lock().await;
            if let Some(sender) = map.remove(&id_val) {
                if message.get("error").is_some() {
                    let _ = sender.send(Err(anyhow::anyhow!("LSP Error: {:?}", message["error"])));
                } else {
                    let _ = sender.send(Ok(message));
                }
            }
        }
    }

    /// Shut down gracefully, then kill and reap a server that does not exit.
    pub async fn shutdown(&self) -> Result<()> {
        use std::time::Duration;
        let _ = self.send_request_timeout("shutdown", Value::Null, 1).await;
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            self.send_notification("exit", Value::Null),
        )
        .await;
        if let Some(mut child) = self.child.lock().await.take() {
            match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
                Ok(result) => {
                    result.context("Failed to reap Tinymist")?;
                }
                Err(_) => {
                    tokio::time::timeout(Duration::from_secs(2), child.kill())
                        .await
                        .context("Timed out killing Tinymist")?
                        .context("Failed to kill Tinymist")?;
                }
            }
        }
        self.pending_requests.lock().await.clear();
        Ok(())
    }
}

// ============================================================================
// LSP Protocol Layer
// ============================================================================

impl TinymistProxy {
    async fn initialize(&self, root_uri: Option<Url>) -> Result<()> {
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "clientInfo": {
                "name": "knot-lsp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "textDocument": {
                    "diagnostic": {},
                    "publishDiagnostics": { "versionSupport": true },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "completion": {},
                }
            },
            "rootUri": root_uri,
        });

        let response = self.send_request("initialize", init_params).await?;
        if response.get("result").is_none() {
            anyhow::bail!("tinymist initialize failed: {:?}", response);
        }

        self.send_notification("initialized", serde_json::json!({}))
            .await?;
        Ok(())
    }

    pub async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        self.send_request_timeout(method, params, 10).await
    }

    pub async fn send_request_timeout(
        &self,
        method: &str,
        params: Value,
        timeout_secs: u64,
    ) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending_requests.lock().await;
            map.insert(id, tx);
        }

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Include writing in the deadline: a stopped server can fill the pipe.
        let response = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
            self.write_message(&request).await?;
            rx.await.context("Tinymist connection closed")?
        })
        .await;
        self.pending_requests.lock().await.remove(&id);
        response.with_context(|| {
            format!("Tinymist request '{method}' timed out after {timeout_secs}s")
        })?
    }

    /// Send a raw JSON-RPC response back to tinymist (e.g. to acknowledge a
    /// server-initiated request such as `window/showDocument`).
    pub async fn send_raw_response(&self, id: &Value, result: Value) -> Result<()> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        self.write_message(&response).await
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&notification).await
    }

    async fn write_message(&self, message: &Value) -> Result<()> {
        let mut dst = BytesMut::new();
        let mut codec = LspCodec;
        codec.encode(message.clone(), &mut dst)?;

        // Log the raw message for debugging
        if let Ok(json) = serde_json::to_string(message) {
            log::info!("LSP OUT: {}", json);
        }

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&dst).await?;
        stdin.flush().await?;
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp_methods::text_document as lsp;

    #[tokio::test]
    async fn project_path_overrides_client_path_without_silent_fallback() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("knot.toml"),
            "[tools]\ntinymist = './missing-tinymist'\n",
        )
        .unwrap();
        let result = TinymistProxy::spawn(
            Some(Url::from_directory_path(dir.path()).unwrap()),
            Some(std::env::current_exe().unwrap()),
        )
        .await;
        let error = result
            .err()
            .expect("invalid project path must fail before starting a process");
        assert!(
            format!("{error:#}").contains("missing-tinymist"),
            "{error:#}"
        );
    }

    #[tokio::test]
    #[ignore = "requires Tinymist 0.15.8 on PATH"]
    async fn tinymist_initializes_and_shuts_down() {
        let dir = tempfile::tempdir().unwrap();
        let (proxy, _notifications) =
            TinymistProxy::spawn(Some(Url::from_directory_path(dir.path()).unwrap()), None)
                .await
                .expect("Tinymist must be installed and initialize successfully");
        tokio::time::timeout(std::time::Duration::from_secs(6), proxy.shutdown())
            .await
            .expect("shutdown exceeded its deadline")
            .expect("shutdown failed");
        assert!(proxy.child.lock().await.is_none());
    }

    #[tokio::test]
    #[ignore = "requires Tinymist 0.15.8 on PATH"]
    async fn tinymist_reports_and_clears_document_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("document with spaces.typ");
        std::fs::write(&file, "= Hello").unwrap();
        let uri = Url::from_file_path(&file).unwrap();
        let (proxy, mut notifications) =
            TinymistProxy::spawn(Some(Url::from_directory_path(dir.path()).unwrap()), None)
                .await
                .expect("Tinymist must be installed and initialize successfully");
        // Always shut down before reporting a failed exchange.
        let exchange = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            proxy
                .send_notification(
                    lsp::DID_OPEN,
                    serde_json::json!({
                        "textDocument": {"uri": uri, "languageId": "typst", "version": 1,
                            "text": "#knot_undefined_symbol"}
                    }),
                )
                .await?;
            let first = diagnostics(&proxy, &mut notifications, &uri, 1).await?;
            anyhow::ensure!(
                first.iter().any(|d| d["message"]
                    .as_str()
                    .is_some_and(|s| s.contains("unknown variable"))),
                "Expected an unknown-variable diagnostic, got {first:?}"
            );
            proxy
                .send_notification(
                    lsp::DID_CHANGE,
                    serde_json::json!({
                        "textDocument": {"uri": uri, "version": 2},
                        "contentChanges": [{"text": "= Hello"}]
                    }),
                )
                .await?;
            let second = diagnostics(&proxy, &mut notifications, &uri, 2).await?;
            anyhow::ensure!(
                second.is_empty(),
                "Diagnostics were not cleared: {second:?}"
            );
            proxy
                .send_notification(
                    lsp::DID_CLOSE,
                    serde_json::json!({"textDocument": {"uri": uri}}),
                )
                .await
        })
        .await;
        proxy.shutdown().await.expect("shutdown failed");
        exchange
            .expect("Tinymist did not complete the document exchange within 30s")
            .expect("Tinymist document exchange failed");
    }

    async fn diagnostics(
        proxy: &TinymistProxy,
        rx: &mut mpsc::Receiver<Value>,
        uri: &Url,
        version: i64,
    ) -> Result<Vec<Value>> {
        while let Some(message) = rx.recv().await {
            if let Some(id) = message.get("id") {
                proxy.send_raw_response(id, Value::Null).await?;
            }
            // Tinymist 0.15.8 omits the optional diagnostic version. Exchanges
            // are sequential; still reject obsolete versions if supplied.
            if message["method"] == lsp::PUBLISH_DIAGNOSTICS
                && message["params"]["uri"] == uri.as_str()
                && (message["params"]["version"].is_null()
                    || message["params"]["version"] == version)
            {
                return message["params"]["diagnostics"]
                    .as_array()
                    .cloned()
                    .context("Missing diagnostics array");
            }
        }
        anyhow::bail!("Tinymist closed the notification stream before version {version}")
    }

    #[tokio::test]
    async fn unresponsive_server_times_out_and_is_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("silent-server.exe");
        let build = std::process::Command::new("rustc")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/silent_server.rs"
            ))
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let proxy = TinymistProxy {
            stdin: Arc::new(Mutex::new(child.stdin.take().unwrap())),
            child: Arc::new(Mutex::new(Some(child))),
            request_id: Arc::new(AtomicU64::new(1)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        };
        let result = proxy
            .send_request_timeout("fixture/no-response", Value::Null, 1)
            .await;
        let blocked_write = proxy
            .send_request_timeout(
                "fixture/blocked-write",
                Value::String("x".repeat(8 * 1024 * 1024)),
                1,
            )
            .await;
        let shutdown =
            tokio::time::timeout(std::time::Duration::from_secs(6), proxy.shutdown()).await;
        shutdown
            .expect("shutdown hung")
            .expect("forced shutdown failed");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("fixture/no-response")
        );
        assert!(
            blocked_write
                .unwrap_err()
                .to_string()
                .contains("fixture/blocked-write")
        );
        assert!(proxy.pending_requests.lock().await.is_empty());
        assert!(proxy.child.lock().await.is_none());
    }
}
