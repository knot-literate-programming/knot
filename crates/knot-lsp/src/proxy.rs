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
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

/// A proxy to a tinymist LSP subprocess
pub struct TinymistProxy {
    /// Writer to send data to tinymist's stdin
    stdin: ChildStdin,
    /// Handle to the child process (kept for shutdown)
    child: Option<Child>,
    /// Counter for request IDs
    request_id: Arc<AtomicU64>,
    /// Map of pending requests (ID -> Sender)
    /// When we send a request, we store a sender here.
    /// The background reader task will look it up when a response arrives.
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
}

// ============================================================================
// Subprocess Management
// ============================================================================

impl TinymistProxy {
    /// Spawn a new tinymist subprocess
    ///
    /// # Returns
    /// * `Ok((proxy, notification_receiver))` - Proxy for sending requests + Receiver for notifications
    /// * `Err(_)` - Failed to spawn or initialize
    pub async fn spawn() -> Result<(Self, mpsc::Receiver<Value>)> {
        // Try to find tinymist in PATH
        let tinymist_path = which::which("tinymist")
            .context("tinymist not found in PATH. Install from: https://github.com/Myriad-Dreamin/tinymist")?;

        // Spawn tinymist with stdio transport
        let mut child = Command::new(&tinymist_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Forward stderr for debugging
            .kill_on_drop(true) // Ensure child is killed if handle is dropped
            .spawn()
            .context("Failed to spawn tinymist process")?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stdout_reader = BufReader::new(stdout);

        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, notification_rx) = mpsc::channel(100);

        // Spawn background task to read from tinymist
        let pending_requests_clone = pending_requests.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::read_loop(stdout_reader, pending_requests_clone, notification_tx).await {
                eprintln!("Tinymist read loop error: {}", e);
            }
        });

        let mut proxy = Self {
            stdin,
            child: Some(child),
            request_id: Arc::new(AtomicU64::new(1)),
            pending_requests,
        };

        // Send initialize request
        proxy.initialize().await?;

        Ok((proxy, notification_rx))
    }

    /// Shutdown the tinymist subprocess gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        // Send shutdown request
        // We ignore the result because we're shutting down anyway
        let _ = self.send_request("shutdown", Value::Null).await;

        // Send exit notification
        let _ = self.send_notification("exit", Value::Null).await;

        // Wait for process to exit
        if let Some(mut child) = self.child.take() {
            // Wait with a timeout to avoid hanging forever
            let _ = tokio::time::timeout(std::time::Duration::from_millis(1000), child.wait()).await;
        }

        Ok(())
    }
}

// ============================================================================
// Reading Loop (Background Task)
// ============================================================================

impl TinymistProxy {
    /// Continuous loop that reads JSON-RPC messages from tinymist's stdout
    async fn read_loop<R: AsyncReadExt + Unpin>(
        mut reader: BufReader<R>,
        pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
        notification_tx: mpsc::Sender<Value>,
    ) -> Result<()> {
        let mut line = String::new();

        loop {
            // 1. Read Headers (Content-Length)
            let mut content_length: Option<usize> = None;
            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line).await?;
                if bytes_read == 0 {
                    return Ok(()); // EOF
                }

                // Check for end of headers (empty line)
                if line == "\r\n" {
                    break;
                }

                if line.starts_with("Content-Length: ") {
                    let len_str = line.trim_start_matches("Content-Length: ").trim();
                    content_length = Some(len_str.parse().context("Invalid Content-Length")?);
                }
            }

            let content_length = content_length.context("Missing Content-Length header")?;

            // 2. Read Content
            let mut content_bytes = vec![0u8; content_length];
            reader.read_exact(&mut content_bytes).await?;
            
            let content = String::from_utf8(content_bytes).context("Invalid UTF-8 in message")?;
            let message: Value = serde_json::from_str(&content).context("Invalid JSON in message")?;

            // 3. Dispatch Message
            if let Some(id) = message.get("id") {
                // It's a Request or Response
                if message.get("method").is_some() {
                    // It's a Request FROM server (e.g. workspace/configuration)
                    // We don't support handling requests from tinymist yet
                    // Just log or ignore
                } else {
                    // It's a Response to our request
                    if let Some(id_val) = id.as_u64() {
                        let mut map = pending_requests.lock().await;
                        if let Some(sender) = map.remove(&id_val) {
                            // Determine if it's a success or error response
                            if message.get("error").is_some() {
                                let _ = sender.send(Err(anyhow::anyhow!("LSP Error: {:?}", message["error"])));
                            } else {
                                let _ = sender.send(Ok(message));
                            }
                        }
                    }
                }
            } else {
                // It's a Notification (no id)
                // Forward to main loop
                let _ = notification_tx.send(message).await;
            }
        }
    }
}

// ============================================================================
// LSP Protocol Layer
// ============================================================================

impl TinymistProxy {
    /// Send the LSP initialize request to tinymist
    async fn initialize(&mut self) -> Result<()> {
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
            "rootUri": null,
        });

        let response = self.send_request("initialize", init_params).await?;

        // Verify we got a successful response
        if response.get("result").is_none() {
            anyhow::bail!("tinymist initialize failed: {:?}", response);
        }

        // Send initialized notification
        self.send_notification("initialized", serde_json::json!({})).await?;

        Ok(())
    }

    /// Send an LSP request to tinymist and wait for the response
    ///
    /// # Arguments
    /// * `method` - LSP method name (e.g., "textDocument/hover")
    /// * `params` - Request parameters as JSON
    ///
    /// # Returns
    /// The JSON-RPC response from tinymist
    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        // Register pending request
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

        // Write request
        if let Err(e) = self.write_message(&request).await {
            // Clean up if write fails
            let mut map = self.pending_requests.lock().await;
            map.remove(&id);
            return Err(e);
        }

        // Wait for response
        match rx.await {
            Ok(result) => result,
            Err(_) => {
                // Sender dropped (likely process died or read loop crashed)
                // Clean up map just in case (though it should be gone)
                let mut map = self.pending_requests.lock().await;
                map.remove(&id);
                Err(anyhow::anyhow!("Tinymist connection closed"))
            }
        }
    }

    /// Send an LSP notification to tinymist (no response expected)
    ///
    /// # Arguments
    /// * `method` - LSP method name (e.g., "textDocument/didChange")
    /// * `params` - Notification parameters as JSON
    pub async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&notification).await
    }

    /// Write a JSON-RPC message with LSP headers
    async fn write_message(&mut self, message: &Value) -> Result<()> {
        let content = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin
            .write_all(header.as_bytes())
            .await
            .context("Failed to write header")?;
        self.stdin
            .write_all(content.as_bytes())
            .await
            .context("Failed to write content")?;
        self.stdin.flush().await.context("Failed to flush")?;

        Ok(())
    }
}

// ============================================================================
// Lifecycle
// ============================================================================

// Note: Drop impl removed because shutdown() is async and cannot be called from Drop.
// Users must explicitly call shutdown() before dropping TinymistProxy.
// The tokio::process::Child will be killed automatically on drop.

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run if tinymist is installed
    async fn test_spawn_tinymist() {
        let proxy = TinymistProxy::spawn().await;
        match proxy {
            Ok(mut p) => {
                println!("tinymist spawned successfully");
                let _ = p.shutdown().await;
            }
            Err(e) => {
                eprintln!("tinymist not available: {}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Only run if tinymist is installed
    async fn test_send_notification() {
        let mut proxy = match TinymistProxy::spawn().await {
            Ok(p) => p,
            Err(_) => {
                eprintln!("tinymist not available, skipping test");
                return;
            }
        };

        // Send a didOpen notification
        let result = proxy
            .send_notification(
                "textDocument/didOpen",
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
