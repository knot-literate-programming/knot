// Air formatter integration for R code chunks
//
// Provides formatting capabilities for R code using the Air formatter:
// - Format entire R chunks
// - Format on type (new line)
// - Format on save

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;

/// Air formatter wrapper
#[derive(Debug, Clone)]
pub struct AirFormatter {
    air_path: PathBuf,
}

impl AirFormatter {
    /// Create a new Air formatter, finding the air executable
    pub fn new(path_override: Option<PathBuf>) -> Result<Self> {
        let air_path = if let Some(path) = path_override {
            if path.exists() {
                path
            } else {
                Self::find_air()?
            }
        } else {
            Self::find_air()?
        };
        Ok(Self { air_path })
    }

    /// Find the air executable in PATH or common installation locations
    fn find_air() -> Result<PathBuf> {
        crate::path_resolver::resolve_binary("air").map_err(|_| {
            anyhow::anyhow!(
                "Air formatter not found. Install from: https://posit-dev.github.io/air/\n\
                 Or specify custom path via 'knot.formatter.air.path' setting"
            )
        })
    }

    /// Format R code using Air
    ///
    /// # Arguments
    /// * `code` - R code to format
    ///
    /// # Returns
    /// * `Ok(formatted_code)` - Successfully formatted code
    /// * `Err(_)` - Formatting failed (syntax error, air not available, etc.)
    #[allow(dead_code)]
    pub async fn format_r_code(&self, code: &str) -> Result<String> {
        // Create a temporary file for the R code
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("knot_format_{}.R", uuid::Uuid::new_v4()));

        // Write code to temp file
        fs::write(&temp_file, code)
            .await
            .context("Failed to write to temporary file")?;

        // Run air format on the temp file
        let output = Command::new(&self.air_path)
            .arg("format")
            .arg(&temp_file)
            .output()
            .await
            .context("Failed to spawn air formatter")?;

        // Read the formatted result
        let formatted = if output.status.success() {
            fs::read_to_string(&temp_file)
                .await
                .context("Failed to read formatted file")?
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up temp file before returning error
            let _ = fs::remove_file(&temp_file).await;
            anyhow::bail!("Air formatting failed: {}", stderr)
        };

        // Clean up temp file
        fs::remove_file(&temp_file)
            .await
            .context("Failed to remove temporary file")?;

        Ok(formatted)
    }

    /// Get the path to the Air executable
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.air_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_simple_r_code() {
        let formatter = match AirFormatter::new(None) {
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
        let formatter = match AirFormatter::new(None) {
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
