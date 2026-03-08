// Python Code Execution Logic
//
// Handles two types of Python code execution:
// 1. Full chunk execution (with rich output support)
// 2. Inline expression execution (formatted for inline display)
//
// Uses side-channel (via KNOT_METADATA_FILE) for robust communication.
// If no metadata is provided, stdout text is used as fallback.

use super::process::PythonProcess;
use crate::executors::path_utils::escape_path_for_code;
use crate::executors::{ExecutionAttempt, GraphicsOptions, SideChannel};
use anyhow::{Context, Result};
use std::path::Path;

/// Wrap Python code to capture structured errors and warnings.
///
/// The user code is read from `code_file` at runtime.
/// Using compile() inside the try-except block ensures syntax errors
/// are also captured as structured metadata.
///
/// NOTE: If you change this wrapper, update the traceback_skip value
/// in the execute() call below (currently 1 — skips the internal exec() frame).
fn wrap_python_code_file(code_file: &str) -> String {
    format!(
        r#"import warnings as _knot_wm
import traceback as _knot_tb
with _knot_wm.catch_warnings(record=True) as _knot_caught:
    _knot_wm.simplefilter("always")
    try:
        with open('{code_file}', 'r', encoding='utf-8') as _knot_f:
            _knot_c = compile(_knot_f.read(), '{code_file}', 'exec')
            exec(_knot_c, globals())
    except Exception as _knot_e:
        for _w in _knot_caught:
            _knot_add_warning(str(_w.message), line=_w.lineno)
        _write_metadata({{'message': f"{{type(_knot_e).__name__}}: {{str(_knot_e)}}", 'traceback': _knot_tb.format_tb(_knot_e.__traceback__)}}, type='error')
        raise
for _w in _knot_caught:
    _knot_add_warning(str(_w.message), line=_w.lineno)
_write_metadata(None, type='sync')
"#
    )
}

/// Executes a code chunk in the persistent Python process
pub fn execute(
    process: &mut PythonProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions,
) -> Result<ExecutionAttempt> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;

    // Write user code to a temp file — Python reads it via compile() so no
    // escaping is needed and syntax errors are caught inside try-except.
    let code_file = tempfile::Builder::new()
        .prefix("knot_code_")
        .suffix(".py")
        .tempfile()
        .context("Failed to create temp file for Python code")?;
    std::fs::write(code_file.path(), code).context("Failed to write Python code to temp file")?;
    let code_file_str = escape_path_for_code(code_file.path());

    // Set environment variables via setup_environment() function
    let meta_file = escape_path_for_code(channel.path());
    let cache_dir_str = escape_path_for_code(cache_dir);

    let setup_code = format!(
        "setup_environment('{}', '{}', {}, {}, {}, '{}')",
        meta_file, cache_dir_str, graphics.width, graphics.height, graphics.dpi, graphics.format
    );

    process.execute_code(&setup_code)?;
    let _ = process.read_until_boundary()?;

    // Wrap the user code with error/warning handlers
    let wrapped = wrap_python_code_file(&code_file_str);
    process.execute_code(&wrapped)?;
    let (stdout, stderr) = process.read_until_boundary()?;
    // code_file is dropped here — temp file cleaned up automatically

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;

    // Python wrapper frames to skip: 1 (skips the internal exec() frame from wrap_python_code_file)
    crate::executors::process_execution_output(code, metadata, &stdout, &stderr, 1)
}
