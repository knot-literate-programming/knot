// R Code Execution Logic
//
// Handles two types of R code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.
// If no metadata is provided, stdout text is used as fallback.

use super::{RExecutor, formatters, process::RProcess};
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{ExecutionAttempt, GraphicsOptions, SideChannel};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Executes a code chunk in the persistent R process
pub fn execute(
    process: &mut RProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionAttempt> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

    // Write user code to a temp file — R reads it via parse(file=...) so no
    // escaping is needed and syntax errors are caught inside tryCatch.
    let code_file = tempfile::Builder::new()
        .prefix("knot_code_")
        .suffix(".R")
        .tempfile()
        .context("Failed to create temp file for R code")?;
    std::fs::write(code_file.path(), code).context("Failed to write R code to temp file")?;
    let code_file_str = escape_path_for_code(code_file.path());

    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Set environment variables via setup_environment() function
    let meta_file = escape_path_for_code(channel.path());
    let cache_dir_str = escape_path_for_code(cache_dir);
    writeln!(
        stdin,
        "setup_environment('{}', '{}', {}, {}, {}, '{}')",
        meta_file, cache_dir_str, graphics.width, graphics.height, graphics.dpi, graphics.format
    )?;

    // Send the source call and finish the block with END_EXEC.
    // The R-side knot_main_loop handles the tryCatch/eval/boundary logic.
    writeln!(stdin, "source('{}', local=FALSE, echo=FALSE)", code_file_str)?;
    writeln!(stdin, "END_EXEC")?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;
    // code_file is dropped here — temp file cleaned up automatically

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;

    // R knot_main_loop adds 4 frames: tryCatch, withCallingHandlers, withVisible, eval
    crate::executors::process_execution_output(code, metadata, &stdout_output, &stderr_output, 4)
}

/// Execute an inline R expression and return formatted result
pub fn execute_inline(executor: &mut RExecutor, code: &str) -> Result<String> {
    // For inline expressions, we use a simpler approach without the structured wrapper
    // because they don't need rich output/warnings capture and it avoids side-channel issues.
    let stdin = executor
        .process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // knot_main_loop handles visibility and printing
    writeln!(stdin, "{}", code)?;
    writeln!(stdin, "END_EXEC")?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = executor.process.read_until_boundary()?;

    if !stderr_output.trim().is_empty() && stderr_output.to_lowercase().contains("error") {
        anyhow::bail!("Inline R execution failed: {}", stderr_output.trim());
    }

    formatters::format_inline_output(&stdout_output)
}

/// Execute lightweight R code and return raw stdout
pub fn query(process: &mut RProcess, code: &str) -> Result<String> {
    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Write the code, followed by END_EXEC
    writeln!(stdin, "{}", code)?;
    writeln!(stdin, "END_EXEC")?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;

    if !stderr_output.trim().is_empty() {
        log::warn!("R query stderr: {}", stderr_output.trim());
    }

    Ok(stdout_output)
}
