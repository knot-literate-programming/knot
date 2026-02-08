// R Code Execution Logic
//
// Handles two types of R code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.
// If no metadata is provided, stdout text is used as fallback.

use super::{BOUNDARY, RExecutor, formatters, process::RProcess};
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{
    ExecutionResult, GraphicsOptions, LanguageExecutor, OutputMetadata, SideChannel,
};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Executes a code chunk in the persistent R process
pub fn execute(
    process: &mut RProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Set environment variables in the R process
    let meta_file = escape_path_for_code(channel.path());
    let cache_dir_str = escape_path_for_code(cache_dir);
    writeln!(stdin, "Sys.setenv(KNOT_METADATA_FILE = '{}')", meta_file)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_WIDTH = '{}')", graphics.width)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_HEIGHT = '{}')", graphics.height)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_DPI = '{}')", graphics.dpi)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_FORMAT = '{}')", graphics.format)?;
    writeln!(stdin, "Sys.setenv(KNOT_CACHE_DIR = '{}')", cache_dir_str)?;

    // Write the code, followed by boundary markers
    writeln!(stdin, "{}", code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;

    // Check if stderr contains actual errors
    if !stderr_output.trim().is_empty() {
        let stderr_lower = stderr_output.to_lowercase();
        let is_error = stderr_lower.contains("error")
            || stderr_lower.contains("erreur")
            || stderr_lower.contains("execution arrêtée")
            || stderr_lower.contains("execution halted")
            || stderr_lower.contains("could not find function")
            || stderr_lower.contains("objet") && stderr_lower.contains("introuvable");

        if is_error {
            let code_preview = if code.lines().count() > 5 {
                let lines: Vec<&str> = code.lines().take(5).collect();
                format!(
                    "{}\n... ({} lines truncated)",
                    lines.join("\n"),
                    code.lines().count() - 5
                )
            } else {
                code.to_string()
            };

            anyhow::bail!(
                "R execution failed.\n\nCode:\n{}\n\nError:\n{}",
                code_preview,
                stderr_output.trim()
            );
        } else {
            log::warn!("R stderr (informational): {}", stderr_output.trim());
        }
    }

    // Read metadata from side-channel and convert to ExecutionResult
    let metadata = channel.read_metadata()?;
    metadata_to_execution_result(metadata, &stdout_output)
}

/// Execute an inline R expression and return formatted result
pub fn execute_inline(executor: &mut RExecutor, code: &str) -> Result<String> {
    let defaults = crate::parser::ChunkOptions::default_resolved();
    let graphics = GraphicsOptions {
        width: defaults.fig_width,
        height: defaults.fig_height,
        dpi: defaults.dpi,
        format: defaults.fig_format,
    };

    // Correctly call trait method
    let result = LanguageExecutor::execute(executor, code, &graphics)?;

    let output = match result {
        ExecutionResult::Text(text) => text,
        _ => anyhow::bail!("Complex outputs are not supported in inline expressions."),
    };

    formatters::format_inline_output(&output)
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

/// Convert side-channel metadata to ExecutionResult
fn metadata_to_execution_result(
    metadata: Vec<OutputMetadata>,
    stdout_text: &str,
) -> Result<ExecutionResult> {
    let mut text_content = String::new();
    let mut plot_path: Option<PathBuf> = None;
    let mut dataframe_path: Option<PathBuf> = None;

    for item in metadata {
        match item {
            OutputMetadata::Text { content } => {
                if !text_content.is_empty() {
                    text_content.push('\n');
                }
                text_content.push_str(&content);
            }
            OutputMetadata::Plot { path, .. } => {
                plot_path = Some(path);
            }
            OutputMetadata::DataFrame { path } => {
                dataframe_path = Some(path);
            }
        }
    }

    if text_content.is_empty() && !stdout_text.trim().is_empty() {
        text_content = stdout_text.to_string();
    }

    match (text_content.is_empty(), dataframe_path, plot_path) {
        (false, None, None) => Ok(ExecutionResult::Text(text_content)),
        (_, Some(df), None) => Ok(ExecutionResult::DataFrame(df)),
        (false, None, Some(plot)) => Ok(ExecutionResult::TextAndPlot {
            text: text_content,
            plot,
        }),
        (_, None, Some(plot)) => Ok(ExecutionResult::Plot(plot)),
        (_, Some(df), Some(plot)) => Ok(ExecutionResult::DataFrameAndPlot {
            dataframe: df,
            plot,
        }),
        (true, None, None) => Ok(ExecutionResult::Text(String::new())),
    }
}
