//! External Code Formatters
//!
//! Provides integration with external formatting tools:
//! - R: Air (https://posit-dev.github.io/air/)
//! - Python: Ruff (https://github.com/astral-sh/ruff)

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Formats R code using the 'air' formatter
pub fn format_r(code: &str) -> Result<String> {
    // Air currently prefers working on files
    let temp_dir = std::env::temp_dir();
    let uuid = uuid::Uuid::new_v4();
    let temp_file = temp_dir.join(format!("knot_fmt_{}.R", uuid));

    std::fs::write(&temp_file, code).context("Failed to write R code to temp file")?;

    let output = Command::new("air")
        .arg("format")
        .arg(&temp_file)
        .output()
        .context("Failed to execute 'air'. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&temp_file);
        anyhow::bail!("Air formatting failed: {}", stderr);
    }

    let formatted = std::fs::read_to_string(&temp_file).context("Failed to read formatted R code")?;
    let _ = std::fs::remove_file(&temp_file);

    Ok(formatted)
}

/// Formats Python code using the 'ruff' formatter
pub fn format_python(code: &str) -> Result<String> {
    // Ruff supports stdin/stdout via 'format -'
    let mut child = Command::new("ruff")
        .arg("format")
        .arg("-")
        .arg("--stdin-filename")
        .arg("chunk.py")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to execute 'ruff'. Is it installed?")?;

    let mut stdin = child.stdin.take().context("Failed to open ruff stdin")?;
    let code_to_send = code.to_string();
    
    std::thread::spawn(move || {
        let _ = stdin.write_all(code_to_send.as_bytes());
    });

    let output = child.wait_with_output().context("Failed to read ruff output")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Ruff formatting failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Helper to format code based on language
pub fn format_code(code: &str, lang: &str) -> Result<String> {
    match lang {
        "r" => format_r(code),
        "python" => format_python(code),
        _ => Ok(code.to_string()), // No-op for other languages
    }
}
