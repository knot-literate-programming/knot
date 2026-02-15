//! Code formatters: Air (R) and Ruff (Python)
//!
//! The [`CodeFormatter`] struct holds pre-resolved binary paths so discovery
//! happens once at startup.  When a path is `None` the binary is looked up on
//! `PATH` at each invocation.
//!
//! This module is intentionally **synchronous** so that knot-core stays free
//! of any async runtime dependency.  Callers that need async execution (e.g.
//! the LSP) should wrap calls with `tokio::task::spawn_blocking`.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Wraps Air (R) and Ruff (Python) formatters.
///
/// Both paths are optional: if a binary is not found the corresponding
/// language falls back to invoking by bare name (PATH lookup).
#[derive(Debug, Clone)]
pub struct CodeFormatter {
    air_path: Option<PathBuf>,
    ruff_path: Option<PathBuf>,
}

impl CodeFormatter {
    /// Create a new formatter.
    /// Pass `None` for either path to rely on PATH lookup at invocation time.
    pub fn new(air_path: Option<PathBuf>, ruff_path: Option<PathBuf>) -> Self {
        Self {
            air_path,
            ruff_path,
        }
    }

    /// Format `code` by language ("r" or "python").
    /// Returns `Err` if the formatter binary is unavailable or reports a failure.
    /// Other languages are returned unchanged.
    pub fn format_code(&self, code: &str, lang: &str) -> Result<String> {
        match lang {
            "r" => self.format_r(code),
            "python" => self.format_python(code),
            _ => Ok(code.to_string()),
        }
    }

    fn air_command(&self) -> Command {
        match &self.air_path {
            Some(p) => Command::new(p),
            None => Command::new("air"),
        }
    }

    fn ruff_command(&self) -> Command {
        match &self.ruff_path {
            Some(p) => Command::new(p),
            None => Command::new("ruff"),
        }
    }

    fn format_r(&self, code: &str) -> Result<String> {
        let temp_file = std::env::temp_dir().join(format!("knot_fmt_{}.R", uuid::Uuid::new_v4()));

        std::fs::write(&temp_file, code).context("Failed to write R code to temp file")?;

        let output = self
            .air_command()
            .arg("format")
            .arg(&temp_file)
            .output()
            .context("Failed to execute 'air'. Is it installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = std::fs::remove_file(&temp_file);
            anyhow::bail!("Air formatting failed: {}", stderr);
        }

        let formatted =
            std::fs::read_to_string(&temp_file).context("Failed to read formatted R code")?;
        let _ = std::fs::remove_file(&temp_file);
        Ok(formatted)
    }

    fn format_python(&self, code: &str) -> Result<String> {
        let mut child = self
            .ruff_command()
            .arg("format")
            .arg("-")
            .arg("--stdin-filename")
            .arg("chunk.py")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to execute 'ruff'. Is it installed?")?;

        // Write stdin in a thread to avoid a deadlock when the pipe buffer fills.
        let mut stdin = child.stdin.take().context("Failed to open ruff stdin")?;
        let code_to_send = code.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(code_to_send.as_bytes());
        });

        let output = child
            .wait_with_output()
            .context("Failed to read ruff output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Ruff formatting failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Convenience wrapper: format with PATH lookup (no pre-resolved paths).
/// Prefer constructing a [`CodeFormatter`] once and reusing it.
pub fn format_code(code: &str, lang: &str) -> Result<String> {
    CodeFormatter::new(None, None).format_code(code, lang)
}
