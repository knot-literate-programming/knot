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
use crate::executors::{ExecutionOutput, ExecutionResult, GraphicsOptions, LanguageExecutor, SideChannel};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Wrap R code with error handlers to capture structured errors and warnings.
///
/// NOTE: If you change this wrapper, you MUST update the line offset
/// in the error reporting logic.
fn wrap_r_code(code: &str) -> String {
    format!(
        r#".knot_run <- function() {{
  withCallingHandlers(
    tryCatch({{
{code}
      # Final sync of metadata (including warnings) on success
      invisible(.write_metadata(NULL, type = "sync"))
    }}, error = function(e) {{
      err_obj <- list(
        message = e$message,
        call = if(!is.null(e$call)) deparse(e$call)[1] else NULL,
        traceback = as.character(sys.calls())
      )
      .write_metadata(err_obj, type = "error")
      stop(e)
    }}),
    warning = function(w) {{
      .knot_add_warning(w)
      invokeRestart("muffleWarning")
    }}
  )
}}
.knot_run()
rm(.knot_run)
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
        let code_preview = format_code_with_context(code, &error.message, 3);
        
        anyhow::bail!(
            "R execution failed.\n\nCode:\n{}\n\nError: {}\nCall: {}\n\nTraceback:\n{}",
            code_preview,
            error.message,
            error.call.as_deref().unwrap_or("unknown"),
            error.traceback.join("\n")
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
    let defaults = crate::parser::ChunkOptions::default_resolved();
    let graphics = GraphicsOptions {
        width: defaults.fig_width,
        height: defaults.fig_height,
        dpi: defaults.dpi,
        format: defaults.fig_format.as_str().to_string(),
    };

    // Correctly call trait method
    let output = LanguageExecutor::execute(executor, code, &graphics)?;

    let output_text = match output.result {
        ExecutionResult::Text(text) => text,
        _ => anyhow::bail!("Complex outputs are not supported in inline expressions."),
    };

    formatters::format_inline_output(&output_text)
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
