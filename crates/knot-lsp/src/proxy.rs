// Tinymist LSP proxy - subprocess communication
//
// This module manages a tinymist subprocess and forwards LSP requests/responses.
// It handles:
// - Spawning and managing the tinymist process
// - Sending LSP requests via JSON-RPC
// - Receiving LSP responses and notifications
// - Graceful shutdown

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A proxy to a tinymist LSP subprocess
pub struct TinymistProxy {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: Arc<AtomicU64>,
}

// ============================================================================
// Subprocess Management
// ============================================================================

impl TinymistProxy {
    /// Spawn a new tinymist subprocess
    ///
    /// # Returns
    /// * `Ok(proxy)` - Successfully spawned and initialized tinymist
    /// * `Err(_)` - Failed to spawn or initialize
    pub fn spawn() -> Result<Self> {
        // Try to find tinymist in PATH
        let tinymist_path = which::which("tinymist")
            .context("tinymist not found in PATH. Install from: https://github.com/Myriad-Dreamin/tinymist")?;

        // Spawn tinymist with stdio transport
        let mut child = Command::new(&tinymist_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Forward stderr for debugging
            .spawn()
            .context("Failed to spawn tinymist process")?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stdout = BufReader::new(stdout);

        let mut proxy = Self {
            child,
            stdin,
            stdout,
            request_id: Arc::new(AtomicU64::new(1)),
        };

        // Send initialize request
        proxy.initialize()?;

        Ok(proxy)
    }

    /// Shutdown the tinymist subprocess gracefully
    pub fn shutdown(&mut self) -> Result<()> {
        // Send shutdown request
        let _response = self.send_request("shutdown", Value::Null)?;

        // Send exit notification
        self.send_notification("exit", Value::Null)?;

        // Wait for process to exit
        self.child
            .wait()
            .context("Failed to wait for tinymist to exit")?;

        Ok(())
    }
}

// ============================================================================
// LSP Protocol Layer
// ============================================================================

impl TinymistProxy {
    /// Send the LSP initialize request to tinymist
    fn initialize(&mut self) -> Result<()> {
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

        let response = self.send_request("initialize", init_params)?;

        // Verify we got a successful response
        if response.get("result").is_none() {
            anyhow::bail!("tinymist initialize failed: {:?}", response);
        }

        // Send initialized notification
        self.send_notification("initialized", serde_json::json!({}))?;

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
    pub fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Write request
        self.write_message(&request)?;

        // Read response (blocking)
        self.read_response(id)
    }

    /// Send an LSP notification to tinymist (no response expected)
    ///
    /// # Arguments
    /// * `method` - LSP method name (e.g., "textDocument/didChange")
    /// * `params` - Notification parameters as JSON
    pub fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&notification)
    }
}

// ============================================================================
// JSON-RPC Transport Layer
// ============================================================================

impl TinymistProxy {
    /// Write a JSON-RPC message with LSP headers
    fn write_message(&mut self, message: &Value) -> Result<()> {
        let content = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin
            .write_all(header.as_bytes())
            .context("Failed to write header")?;
        self.stdin
            .write_all(content.as_bytes())
            .context("Failed to write content")?;
        self.stdin.flush().context("Failed to flush")?;

        Ok(())
    }

    /// Read a JSON-RPC response with the given ID
    fn read_response(&mut self, expected_id: u64) -> Result<Value> {
        loop {
            let message = self.read_message()?;

            // Check if this is the response we're waiting for
            if let Some(id) = message.get("id") {
                if id.as_u64() == Some(expected_id) {
                    return Ok(message);
                }
            }

            // If it's a different message (notification, different response),
            // we could queue it, but for now we just skip it
            // TODO: Handle notifications from tinymist
        }
    }

    /// Read a single JSON-RPC message from tinymist
    fn read_message(&mut self) -> Result<Value> {
        // Read headers
        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            self.stdout
                .read_line(&mut line)
                .context("Failed to read header line")?;

            if line == "\r\n" {
                break; // End of headers
            }

            if line.starts_with("Content-Length: ") {
                let len_str = line.trim_start_matches("Content-Length: ").trim();
                content_length = Some(
                    len_str
                        .parse()
                        .context("Invalid Content-Length")?,
                );
            }
        }

        let content_length = content_length.context("Missing Content-Length header")?;

        // Read content
        let mut content_bytes = vec![0u8; content_length];
        std::io::Read::read_exact(&mut self.stdout, &mut content_bytes)
            .context("Failed to read message content")?;

        let content = String::from_utf8(content_bytes).context("Invalid UTF-8 in message")?;
        let message: Value = serde_json::from_str(&content).context("Invalid JSON in message")?;

        Ok(message)
    }
}

// ============================================================================
// Lifecycle
// ============================================================================

impl Drop for TinymistProxy {
    fn drop(&mut self) {
        // Try to shutdown gracefully, but don't panic if it fails
        let _ = self.shutdown();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Only run if tinymist is installed
    fn test_spawn_tinymist() {
        let proxy = TinymistProxy::spawn();
        match proxy {
            Ok(mut p) => {
                println!("tinymist spawned successfully");
                let _ = p.shutdown();
            }
            Err(e) => {
                eprintln!("tinymist not available: {}", e);
            }
        }
    }

    #[test]
    #[ignore] // Only run if tinymist is installed
    fn test_send_notification() {
        let mut proxy = match TinymistProxy::spawn() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("tinymist not available, skipping test");
                return;
            }
        };

        // Send a didOpen notification
        let result = proxy.send_notification(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.typ",
                    "languageId": "typst",
                    "version": 1,
                    "text": "= Hello"
                }
            }),
        );

        assert!(result.is_ok(), "Failed to send notification: {:?}", result);
    }
}
