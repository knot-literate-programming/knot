// R Code Execution Logic
//
// Handles two types of R code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.
// If no metadata is provided, stdout text is used as fallback.

use super::{formatters, process::RProcess, RExecutor, BOUNDARY};
use crate::executors::{ExecutionResult, GraphicsOptions, LanguageExecutor, OutputMetadata, SideChannel};
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Executes a code chunk in the persistent R process
pub fn execute(process: &mut RProcess, cache_dir: &Path, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;
    channel.setup_env()?;

    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Set environment variables in the R process
    // We must use Sys.setenv() because the child process environment is independent
    let meta_file = channel.path().to_string_lossy().replace('\\', "\\\\");
    let cache_dir_str = cache_dir.to_string_lossy().replace('\\', "\\\\");
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

    // Read metadata from side-channel and convert to ExecutionResult
    let metadata = channel.read_metadata()?;
    metadata_to_execution_result(metadata, &stdout_output)
}

/// Execute an inline R expression and return formatted result
pub fn execute_inline(executor: &mut RExecutor, code: &str) -> Result<String> {
    // For inline expressions, use default graphics options (not used anyway)
    let graphics = GraphicsOptions {
        width: crate::defaults::Defaults::FIG_WIDTH,
        height: crate::defaults::Defaults::FIG_HEIGHT,
        dpi: crate::defaults::Defaults::DPI,
        format: crate::defaults::Defaults::FIG_FORMAT.to_string(),
    };

    // Execute the code and get output
    let result = executor.execute(code, &graphics)?;

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
/// If metadata is empty, uses stdout text as content.
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

/// Save the current R session to a snapshot file
///
/// Executes `save.image(file)` in the R process to save all objects
/// in the global environment to a .RData file.
pub fn save_session(process: &mut RProcess, snapshot_file: &Path) -> Result<()> {
    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Convert path to string, escaping backslashes for Windows
    let path_str = snapshot_file
        .to_str()
        .context("Invalid path for snapshot file")?
        .replace('\\', "\\\\");

    // Execute save.image()
    writeln!(stdin, "save.image(file = \"{}\")", path_str)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    // Wait for completion by reading until boundary markers
    let (_stdout_output, stderr_output) = process.read_until_boundary()?;

    // Check for errors
    if !stderr_output.trim().is_empty() {
        anyhow::bail!("Failed to save R session: {}", stderr_output.trim());
    }

    log::debug!("💾 Saved R session to: {}", snapshot_file.display());
    Ok(())
}

/// Load an R session from a snapshot file
///
/// Executes `load(file, envir = .GlobalEnv)` in the R process to restore
/// all objects from a previously saved .RData file.
pub fn load_session(process: &mut RProcess, snapshot_file: &Path) -> Result<()> {
    let stdin = process
        .stdin
        .as_mut()
        .context("R process stdin is not available")?;

    // Convert path to string, escaping backslashes for Windows
    let path_str = snapshot_file
        .to_str()
        .context("Invalid path for snapshot file")?
        .replace('\\', "\\\\");

    // Execute load() with envir = .GlobalEnv to load into global environment
    writeln!(stdin, "load(file = \"{}\", envir = .GlobalEnv)", path_str)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    // Wait for completion by reading until boundary markers
    let (_stdout_output, stderr_output) = process.read_until_boundary()?;

    // Check for errors
    if !stderr_output.trim().is_empty() {
        anyhow::bail!("Failed to load R session: {}", stderr_output.trim());
    }

    log::debug!("📂 Loaded R session from: {}", snapshot_file.display());
    Ok(())
}

/// Execute lightweight R code and return raw stdout
///
/// Unlike execute(), this does not use the side-channel and returns the raw output string.
/// Useful for LSP queries (completion, help).
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
        // Log warning but don't fail, return what we have
        log::warn!("R query stderr: {}", stderr_output.trim());
    }

    Ok(stdout_output)
}

