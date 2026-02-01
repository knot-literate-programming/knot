// R Code Execution Logic
//
// Handles two types of R code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication,
// with fallback to stdout parsing for backward compatibility.

use super::{formatters, output_parser, process::RProcess, RExecutor, BOUNDARY};
use crate::executors::{ExecutionResult, LanguageExecutor, OutputMetadata, SideChannel};
use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::thread;

/// Executes a code chunk in the persistent R process
pub fn execute(process: &mut RProcess, cache_dir: &Path, code: &str) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;
    channel.setup_env()?;

    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;
    let stdout = process
        .stdout
        .as_mut()
        .context("R process stdout is not available")?;
    let stderr = process
        .stderr
        .as_mut()
        .context("R process stderr is not available")?;

    // Write the code, followed by boundary markers
    writeln!(stdin, "{}", code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = thread::scope(|s| {
        let stdout_handle = s.spawn(move || {
            let mut output = String::new();
            let mut line_buffer = String::new();
            loop {
                line_buffer.clear();
                let bytes_read = stdout.read_line(&mut line_buffer).unwrap_or(0);
                if bytes_read == 0 {
                    break;
                }
                if line_buffer.trim_end() == BOUNDARY {
                    break;
                }
                output.push_str(&line_buffer);
            }
            output
        });

        let stderr_handle = s.spawn(move || {
            let mut output = String::new();
            let mut line_buffer = String::new();
            loop {
                line_buffer.clear();
                let bytes_read = stderr.read_line(&mut line_buffer).unwrap_or(0);
                if bytes_read == 0 {
                    break;
                }
                if line_buffer.trim_end() == BOUNDARY {
                    break;
                }
                output.push_str(&line_buffer);
            }
            output
        });

        (
            stdout_handle.join().unwrap(),
            stderr_handle.join().unwrap(),
        )
    });

    // Check if stderr contains actual errors (not just warnings/messages)
    if !stderr_output.trim().is_empty() {
        let stderr_lower = stderr_output.to_lowercase();
        let is_error = stderr_lower.contains("error")
            || stderr_lower.contains("erreur")
            || stderr_lower.contains("execution arrêtée")
            || stderr_lower.contains("execution halted")
            || stderr_lower.contains("could not find function")
            || stderr_lower.contains("objet") && stderr_lower.contains("introuvable");

        if is_error {
            anyhow::bail!(
                "R execution failed:\n\n--- Code ---\n{}\n\n--- Stderr ---\n{}\n\n--- Stdout ---\n{}",
                code,
                stderr_output.trim(),
                stdout_output.trim()
            );
        } else {
            // Just warnings or informational messages, log them but don't fail
            log::warn!("R stderr (informational): {}", stderr_output.trim());
        }
    }

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;

    // Priority: side-channel metadata > stdout parsing (fallback)
    if !metadata.is_empty() {
        return metadata_to_execution_result(metadata, &stdout_output);
    }

    // Fallback to old stdout parsing for backward compatibility
    output_parser::parse_output(&stdout_output, cache_dir)
}

/// Execute an inline R expression and return formatted result
pub fn execute_inline(executor: &mut RExecutor, code: &str) -> Result<String> {
    // Execute the code and get output
    let result = executor.execute(code)?;

    // Extract text output
    let output = match result {
        ExecutionResult::Text(text) => text,
        ExecutionResult::DataFrame(_) => {
            anyhow::bail!("DataFrames are not supported in inline expressions. Use typst(df) in a chunk instead.")
        }
        ExecutionResult::Plot(_) => {
            anyhow::bail!("Plots are not supported in inline expressions. Use typst(gg) in a chunk instead.")
        }
        ExecutionResult::TextAndPlot { .. } | ExecutionResult::DataFrameAndPlot { .. } => {
            anyhow::bail!("Complex outputs are not supported in inline expressions.")
        }
    };

    formatters::format_inline_output(&output)
}

/// Convert side-channel metadata to ExecutionResult
///
/// Priority: side-channel metadata > stdout parsing (fallback)
fn metadata_to_execution_result(
    metadata: Vec<OutputMetadata>,
    stdout_text: &str,
) -> Result<ExecutionResult> {
    let mut text_content = String::new();
    let mut plot_path: Option<PathBuf> = None;
    let mut dataframe_path: Option<PathBuf> = None;

    // Process metadata from side-channel
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

    // If no metadata from side-channel, use stdout text
    if text_content.is_empty() && !stdout_text.trim().is_empty() {
        text_content = stdout_text.to_string();
    }

    // Build ExecutionResult based on what we have
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

