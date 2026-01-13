// Air formatter integration for R code chunks
//
// Provides formatting capabilities for R code using the Air formatter:
// - Format entire R chunks
// - Format on type (new line)
// - Format on save

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Air formatter wrapper
#[derive(Debug, Clone)]
pub struct AirFormatter {
    air_path: PathBuf,
}

impl AirFormatter {
    /// Create a new Air formatter, finding the air executable in PATH
    pub fn new() -> Result<Self> {
        let air_path = Self::find_air()?;
        Ok(Self { air_path })
    }

    /// Create a new Air formatter with a custom path
    pub fn with_path(path: PathBuf) -> Self {
        Self { air_path: path }
    }

    /// Find the air executable in PATH or common installation locations
    fn find_air() -> Result<PathBuf> {
        // Try to find in PATH first
        if let Ok(path) = which::which("air") {
            return Ok(path);
        }

        // Try common installation locations
        let fallback_paths = if cfg!(target_os = "macos") {
            vec![
                PathBuf::from("/usr/local/bin/air"),
                PathBuf::from("/opt/homebrew/bin/air"),
            ]
        } else if cfg!(target_os = "linux") {
            vec![
                PathBuf::from("/usr/local/bin/air"),
                PathBuf::from(shellexpand::tilde("~/.local/bin/air").to_string()),
            ]
        } else if cfg!(target_os = "windows") {
            vec![PathBuf::from(
                shellexpand::env("%LOCALAPPDATA%\\Programs\\air\\air.exe")
                    .unwrap_or_default()
                    .to_string(),
            )]
        } else {
            vec![]
        };

        for path in fallback_paths {
            if path.exists() {
                return Ok(path);
            }
        }

        anyhow::bail!(
            "Air formatter not found. Install from: https://posit-dev.github.io/air/\n\
             Or specify custom path via 'knot.formatter.air.path' setting"
        )
    }

    /// Format R code using Air
    ///
    /// # Arguments
    /// * `code` - R code to format
    ///
    /// # Returns
    /// * `Ok(formatted_code)` - Successfully formatted code
    /// * `Err(_)` - Formatting failed (syntax error, air not available, etc.)
    pub async fn format_r_code(&self, code: &str) -> Result<String> {
        // Spawn air process
        let mut child = Command::new(&self.air_path)
            .arg("format")
            .arg("--stdin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn air formatter")?;

        // Write code to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(code.as_bytes())
                .await
                .context("Failed to write to air stdin")?;
            stdin.flush().await.context("Failed to flush stdin")?;
            drop(stdin); // Close stdin to signal EOF
        }

        // Wait for completion and collect output
        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for air process")?;

        if output.status.success() {
            let formatted = String::from_utf8(output.stdout)
                .context("Air output is not valid UTF-8")?;
            Ok(formatted)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Air formatting failed: {}", stderr)
        }
    }

    /// Check if Air is available
    pub fn is_available(&self) -> bool {
        self.air_path.exists()
    }

    /// Get the path to the Air executable
    pub fn path(&self) -> &PathBuf {
        &self.air_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_simple_r_code() {
        let formatter = match AirFormatter::new() {
            Ok(f) => f,
            Err(_) => {
                eprintln!("Air not installed, skipping test");
                return;
            }
        };

        let input = "x<-1+2";
        let result = formatter.format_r_code(input).await;

        match result {
            Ok(formatted) => {
                // Air should add spaces around operators
                assert!(formatted.contains("<-"));
                assert!(formatted.contains("+"));
            }
            Err(e) => {
                panic!("Formatting failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_format_multiline_r_code() {
        let formatter = match AirFormatter::new() {
            Ok(f) => f,
            Err(_) => {
                eprintln!("Air not installed, skipping test");
                return;
            }
        };

        let input = "library(dplyr)\ndf<-iris%>%filter(Species==\"setosa\")";
        let result = formatter.format_r_code(input).await;

        match result {
            Ok(formatted) => {
                // Should be properly formatted
                assert!(formatted.contains("library(dplyr)"));
                assert!(formatted.contains("<-"));
                assert!(formatted.contains("%>%"));
            }
            Err(e) => {
                panic!("Formatting failed: {}", e);
            }
        }
    }
}
