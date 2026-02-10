// Python Code Execution Logic
//
// Handles two types of Python code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.

use super::process::PythonProcess;
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{ExecutionResult, GraphicsOptions, SideChannel};
use anyhow::Result;
use std::path::Path;

/// Executes a code chunk in the persistent Python process
pub fn execute(
    process: &mut PythonProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

    // Set environment variables via setup_environment() function
    let meta_file = escape_path_for_code(channel.path());
    let cache_dir_str = escape_path_for_code(cache_dir);

    let setup_code = format!(
        "setup_environment('{}', '{}', {}, {}, {}, '{}')",
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

    // Read metadata from side-channel and convert to ExecutionResult
    let metadata = channel.read_metadata()?;
    crate::executors::metadata_to_execution_result(metadata, &stdout)
}
