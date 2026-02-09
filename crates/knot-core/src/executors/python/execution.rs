// Python Code Execution Logic
//
// Handles two types of Python code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.

use super::process::PythonProcess;
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{ExecutionResult, GraphicsOptions, OutputMetadata, SideChannel};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Executes a code chunk in the persistent Python process
pub fn execute(
    process: &mut PythonProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

    // Set environment variables in the Python process
    let meta_file = escape_path_for_code(channel.path());
    let cache_dir_str = escape_path_for_code(cache_dir);

    let setup_code = format!(
        "import os
os.environ['KNOT_METADATA_FILE'] = '{}'
os.environ['KNOT_CACHE_DIR'] = '{}'
os.environ['KNOT_FIG_WIDTH'] = '{}'
os.environ['KNOT_FIG_HEIGHT'] = '{}'
os.environ['KNOT_FIG_DPI'] = '{}'
os.environ['KNOT_FIG_FORMAT'] = '{}'",
        meta_file, cache_dir_str, graphics.width, graphics.height, graphics.dpi, graphics.format
    );

    process.execute_code(&setup_code)?;
    let _ = process.read_until_boundary()?;

    // Execute the actual code
    process.execute_code(code)?;
    let (stdout, stderr) = process.read_until_boundary()?;

    if !stderr.is_empty() && stderr.to_lowercase().contains("traceback") {
        let code_preview = if code.lines().count() > 5 {
            let lines: Vec<&str> = code.lines().take(5).collect();
            format!(
                "{}
... ({} lines truncated)",
                lines.join(
                    "
"
                ),
                code.lines().count() - 5
            )
        } else {
            code.to_string()
        };

        anyhow::bail!(
            "Python execution failed.

Code:
{}

Error:
{}",
            code_preview,
            stderr.trim()
        );
    }

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;
    metadata_to_execution_result(metadata, &stdout)
}

/// Convert side-channel metadata to ExecutionResult
pub fn metadata_to_execution_result(
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
