// R Code Execution Logic
//
// Handles two types of R code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.
// If no metadata is provided, stdout text is used as fallback.

use super::{BOUNDARY, RExecutor, formatters, process::RProcess};
use crate::executors::error_utils::format_code_with_context;
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{ExecutionOutput, GraphicsOptions, SideChannel};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Wrap R code with error handlers to capture structured errors and warnings.
///
/// NOTE: If you change this wrapper, you MUST update the line offset
/// in the error reporting logic.
fn wrap_r_code(code: &str) -> String {
    format!(
        r#"tryCatch({{
  withCallingHandlers({{
    .knot_res <- withVisible({{
{code}
    }})
    if (.knot_res$visible) print(.knot_res$value)
    invisible(.write_metadata(NULL, type = "sync"))
  }}, warning = function(w) {{
    .knot_add_warning(w)
    invokeRestart("muffleWarning")
  }})
}}, error = function(e) {{
  err_obj <- list(message = e$message)
  if (!is.null(e$call)) err_obj$call <- deparse(e$call)[1]
  err_obj$traceback <- as.list(as.character(sys.calls()))
  .write_metadata(err_obj, type = "error")
  stop(e)
}})
"#
    )
}

/// Executes a code chunk in the persistent R process
pub fn execute(
    process: &mut RProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionOutput> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

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

    // Wrap the code with our error handler
    let wrapped_code = wrap_r_code(code);

    // Write the code, followed by boundary markers
    writeln!(stdin, "{}", wrapped_code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;

    // Check for structured errors first (most precise)
    if let Some(error) = &metadata.error {
        // Line offset calculation:
        // In our wrapper, the user code starts at line 4 of the block.
        // But R error reporting can be tricky with sys.calls().
        // For now, we report the message and the call.
        let error_msg = error.message.as_ref().map(|m| m.to_string()).unwrap_or_else(|| "Unknown R error".to_string());
        let code_preview = format_code_with_context(code, &error_msg, 3);
        
        // Skip the first 3 frames which are knot's own wrappers
        // (tryCatch, withCallingHandlers, withVisible), then keep
        // at most 8 of the remaining frames, preferring the innermost
        // ones (closest to the actual error).
        const MAX_TRACEBACK_FRAMES: usize = 8;
        let user_frames: Vec<&String> = error.traceback.iter().skip(3).collect();
        let traceback_str = if user_frames.len() > MAX_TRACEBACK_FRAMES {
            let omitted = user_frames.len() - MAX_TRACEBACK_FRAMES;
            let tail = &user_frames[user_frames.len() - MAX_TRACEBACK_FRAMES..];
            std::iter::once(format!("... {} frames omitted ...", omitted))
                .chain(tail.iter().map(|s| s.to_string()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            user_frames.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
        };

        anyhow::bail!(
            "R execution failed.\n\nCode:\n{}\n\nError: {}\nCall: {}\n\nTraceback:\n{}",
            code_preview,
            error_msg,
            error.call.as_ref().map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string()),
            traceback_str
        );
    }

    // Fallback: Check if stderr contains actual errors (e.g. syntax errors or setup errors)
    if !stderr_output.trim().is_empty() {
        let stderr_lower = stderr_output.to_lowercase();
        let is_error = stderr_lower.contains("error")
            || stderr_lower.contains("erreur")
            || stderr_lower.contains("execution arrêtée")
            || stderr_lower.contains("execution halted")
            || stderr_lower.contains("could not find function")
            || stderr_lower.contains("objet") && stderr_lower.contains("introuvable");

        if is_error {
            let code_preview = format_code_with_context(code, &stderr_output, 3);

            anyhow::bail!(
                "R execution failed (detected via stderr).\n\nCode:\n{}\n\nError:\n{}",
                code_preview,
                stderr_output.trim()
            );
        } else {
            // It might be just messages or warnings that were also printed to stderr
            log::debug!("R stderr (non-fatal): {}", stderr_output.trim());
        }
    }

    crate::executors::metadata_to_execution_result(metadata, &stdout_output)
}

/// Execute an inline R expression and return formatted result
pub fn execute_inline(executor: &mut RExecutor, code: &str) -> Result<String> {
    // For inline expressions, we use a simpler approach without the structured wrapper
    // because they don't need rich output/warnings capture and it avoids side-channel issues.
    let stdin = executor.process.stdin.as_mut().context("R process stdin is not available")?;
    
    // Just wrap in withVisible to get the output
    let inline_code = format!(
        ".knot_res <- withVisible({{ {} }}); if (.knot_res$visible) print(.knot_res$value);",
        code
    );

    writeln!(stdin, "{}", inline_code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
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

    // Write the code, followed by boundary markers
    writeln!(stdin, "{}", code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;

    if !stderr_output.trim().is_empty() {
        log::warn!("R query stderr: {}", stderr_output.trim());
    }

    Ok(stdout_output)
}
