// Unified code formatter: Air (R) + Ruff (Python)

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Wraps Air (R) and Ruff (Python) formatters.
///
/// Both paths are optional: if a binary is not found the corresponding
/// language is silently skipped in [`CodeFormatter::format_code`].
#[derive(Debug, Clone)]
pub struct CodeFormatter {
    air_path: Option<PathBuf>,
    ruff_path: Option<PathBuf>,
}

impl CodeFormatter {
    /// Discover Air and Ruff from PATH or use explicit overrides.
    /// Always succeeds; individual formatters are `None` when not found.
    pub fn new(air_override: Option<PathBuf>, ruff_override: Option<PathBuf>) -> Self {
        let air_path = air_override
            .filter(|p| p.exists())
            .or_else(|| crate::path_resolver::resolve_binary("air").ok());
        let ruff_path = ruff_override
            .filter(|p| p.exists())
            .or_else(|| crate::path_resolver::resolve_binary("ruff").ok());
        Self {
            air_path,
            ruff_path,
        }
    }

    /// Format `code` for the given `lang` ("r" or "python").
    /// Returns `Err` when the formatter binary is unavailable or reports a failure.
    pub async fn format_code(&self, code: &str, lang: &str) -> Result<String> {
        match lang {
            "r" => self.format_r(code).await,
            "python" => self.format_python(code).await,
            _ => Ok(code.to_string()),
        }
    }

    async fn format_r(&self, code: &str) -> Result<String> {
        let air = self
            .air_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Air formatter not found"))?;

        let temp_file = std::env::temp_dir().join(format!("knot_fmt_{}.R", uuid::Uuid::new_v4()));
        fs::write(&temp_file, code)
            .await
            .context("write R temp file")?;

        let output = Command::new(air)
            .arg("format")
            .arg(&temp_file)
            .output()
            .await
            .context("spawn air")?;

        let result = if output.status.success() {
            fs::read_to_string(&temp_file)
                .await
                .context("read formatted R file")?
        } else {
            let _ = fs::remove_file(&temp_file).await;
            anyhow::bail!("air: {}", String::from_utf8_lossy(&output.stderr));
        };
        let _ = fs::remove_file(&temp_file).await;
        Ok(result)
    }

    async fn format_python(&self, code: &str) -> Result<String> {
        let ruff = self
            .ruff_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Ruff formatter not found"))?;

        let mut child = Command::new(ruff)
            .arg("format")
            .arg("-")
            .arg("--stdin-filename")
            .arg("chunk.py")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ruff")?;

        // Write code and close stdin so ruff knows input is complete.
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(code.as_bytes())
                .await
                .context("write to ruff stdin")?;
        }

        let output = child.wait_with_output().await.context("ruff output")?;

        if !output.status.success() {
            anyhow::bail!("ruff: {}", String::from_utf8_lossy(&output.stderr));
        }
        String::from_utf8(output.stdout).context("ruff output is not valid UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_simple_r_code() {
        let formatter = CodeFormatter::new(None, None);
        if formatter.air_path.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let result = formatter.format_code("x<-1+2", "r").await;
        match result {
            Ok(formatted) => {
                assert!(formatted.contains("<-"));
                assert!(formatted.contains("+"));
            }
            Err(e) => panic!("Formatting failed: {}", e),
        }
    }

    #[tokio::test]
    async fn test_format_multiline_r_code() {
        let formatter = CodeFormatter::new(None, None);
        if formatter.air_path.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let result = formatter
            .format_code(
                "library(dplyr)\ndf<-iris%>%filter(Species==\"setosa\")",
                "r",
            )
            .await;
        match result {
            Ok(formatted) => {
                assert!(formatted.contains("library(dplyr)"));
                assert!(formatted.contains("%>%"));
            }
            Err(e) => panic!("Formatting failed: {}", e),
        }
    }
}
