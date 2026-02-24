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
pub struct TinymistProxy {
    /// Writer to send data to tinymist's stdin
    stdin: ChildStdin,
    /// Handle to the child process (kept for shutdown)
    child: Option<Child>,
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
        let tinymist_path = if let Some(path) = path_override {
            if path.exists() {
                path
            } else {
                crate::path_resolver::resolve_binary("tinymist")?
            }
        } else {
            crate::path_resolver::resolve_binary("tinymist")?
        };

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
                        Self::dispatch_message(message, &pending_requests_clone, &notification_tx)
                            .await;
                    }
                    Err(e) => {
                        eprintln!("Tinymist read error: {}", e);
                        break;
                    }
                }
            }
        });

        let mut proxy = Self {
            stdin,
            child: Some(child),
            request_id: Arc::new(AtomicU64::new(1)),
            pending_requests,
        };

        proxy.initialize(root_uri).await?;

        Ok((proxy, notification_rx))
    }

    async fn dispatch_message(
        message: Value,
        pending_requests: &PendingRequests,
        notification_tx: &mpsc::Sender<Value>,
    ) {
        if let Some(id) = message.get("id") {
            if let Some(id_val) = id.as_u64() {
                let mut map = pending_requests.lock().await;
                if let Some(sender) = map.remove(&id_val) {
                    if message.get("error").is_some() {
                        let _ =
                            sender.send(Err(anyhow::anyhow!("LSP Error: {:?}", message["error"])));
                    } else {
                        let _ = sender.send(Ok(message));
                    }
                }
            }
        } else {
            let _ = notification_tx.send(message).await;
        }
    }

    /// Shutdown the tinymist subprocess gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.send_request("shutdown", Value::Null).await;
        let _ = self.send_notification("exit", Value::Null).await;

        if let Some(mut child) = self.child.take() {
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(1000), child.wait()).await;
        }

        Ok(())
    }
}

// ============================================================================
// LSP Protocol Layer
// ============================================================================

impl TinymistProxy {
    async fn initialize(&mut self, root_uri: Option<Url>) -> Result<()> {
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "clientInfo": {
                "name": "knot-lsp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "textDocument": {
                    "diagnostic": {},
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

    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.send_request_timeout(method, params, 10).await
    }

    pub async fn send_request_timeout(
        &mut self,
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

        if let Err(e) = self.write_message(&request).await {
            let mut map = self.pending_requests.lock().await;
            map.remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow::anyhow!("Tinymist connection closed")),
            Err(_) => {
                let mut map = self.pending_requests.lock().await;
                map.remove(&id);
                Err(anyhow::anyhow!(
                    "Tinymist request '{}' timed out after {timeout_secs}s",
                    method
                ))
            }
        }
    }

    pub async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&notification).await
    }

    async fn write_message(&mut self, message: &Value) -> Result<()> {
        let mut dst = BytesMut::new();
        let mut codec = LspCodec;
        codec.encode(message.clone(), &mut dst)?;
        self.stdin.write_all(&dst).await?;
        self.stdin.flush().await?;
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
    #[ignore] // Only run if tinymist is installed
    async fn test_spawn_tinymist() {
        let result = TinymistProxy::spawn(None, None).await;
        match result {
            Ok((mut proxy, _notification_rx)) => {
                println!("tinymist spawned successfully");
                let _ = proxy.shutdown().await;
            }
            Err(e) => {
                eprintln!("tinymist not available: {}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Only run if tinymist is installed
    async fn test_send_notification() {
        let (mut proxy, _notification_rx) = match TinymistProxy::spawn(None, None).await {
            Ok((p, rx)) => (p, rx),
            Err(_) => {
                eprintln!("tinymist not available, skipping test");
                return;
            }
        };

        // Send a didOpen notification
        let result = proxy
            .send_notification(
                lsp::DID_OPEN,
                serde_json::json!({
                    "textDocument": {
                        "uri": "file:///test.typ",
                        "languageId": "typst",
                        "version": 1,
                        "text": "= Hello"
                    }
                }),
            )
            .await;

        assert!(result.is_ok(), "Failed to send notification: {:?}", result);
    }
}
