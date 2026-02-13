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
use crate::executors::{ExecutionOutput, GraphicsOptions, SideChannel};
use anyhow::Result;
use std::path::Path;

/// Wrap Python code to capture structured errors and warnings.
///
/// The user code is compiled and executed via exec() so that its own
/// indentation is preserved. Warnings are intercepted via the `warnings`
/// module. Errors are caught and written to the side-channel metadata.
///
/// NOTE: If you change this wrapper, update the traceback_skip value
/// in the execute() call below (currently 0 — Python frames are all user frames).
fn wrap_python_code(code: &str) -> String {
    // Escape backslashes and triple-quotes in user code for safe embedding
    let escaped = code.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
    format!(
        r#"import warnings as _knot_wm
import traceback as _knot_tb
with _knot_wm.catch_warnings(record=True) as _knot_caught:
    _knot_wm.simplefilter("always")
    try:
        exec(compile("""{escaped}
""", "<chunk>", "exec"), globals())
    except Exception as _knot_e:
        for _w in _knot_caught:
            _knot_add_warning(str(_w.message))
        _write_metadata({{'message': str(_knot_e), 'traceback': _knot_tb.format_tb(_knot_e.__traceback__)}}, type='error')
        raise
for _w in _knot_caught:
    _knot_add_warning(str(_w.message))
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
) -> Result<ExecutionOutput> {
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

    // Wrap the user code with error/warning handlers
    let wrapped = wrap_python_code(code);
    process.execute_code(&wrapped)?;
    let (stdout, stderr) = process.read_until_boundary()?;

    // Read metadata from side-channel
    let metadata = channel.read_metadata()?;

    // Python wrapper frames to skip: 0 (exec/compile frames are user-visible)
    crate::executors::process_execution_output(code, metadata, &stdout, &stderr, 0)
}
