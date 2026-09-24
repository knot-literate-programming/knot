//! Language executor traits and shared output types.
//!
//! [`LanguageExecutor`] is the low-level trait; [`KnotExecutor`] extends it with
//! session persistence (save/load snapshots) used by the snapshot manager.
//! Concrete implementations live in [`python`] and [`r`].

use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdout};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Shared error formatting utilities for executor implementations.
pub mod error_utils;
mod inline;
pub mod manager;
pub mod path_utils;
pub mod python;
pub mod r;
pub mod side_channel;

/// Concurrently read stdout and stderr until boundary markers are reached.
///
/// Spawns two threads, one per stream, and waits for both with the given timeout.
/// Returns `Some((stdout, stderr, reader_out, reader_err))` on success,
/// or `None` if either stream does not produce a boundary within `timeout`.
/// Time allowed for an interpreter to start: the chunk timeout, but never
/// less than [`Defaults::INTERPRETER_STARTUP_TIMEOUT_SECS`](crate::Defaults).
pub(crate) fn startup_timeout(chunk_timeout: Duration) -> Duration {
    chunk_timeout.max(Duration::from_secs(
        crate::Defaults::INTERPRETER_STARTUP_TIMEOUT_SECS,
    ))
}

pub(crate) fn read_streams_until_boundary(
    stdout: BufReader<ChildStdout>,
    stderr: BufReader<ChildStderr>,
    timeout: Duration,
    boundary: &'static str,
) -> Option<(
    String,
    String,
    BufReader<ChildStdout>,
    BufReader<ChildStderr>,
)> {
    let (tx_out, rx_out) = mpsc::channel::<(String, BufReader<ChildStdout>)>();
    let (tx_err, rx_err) = mpsc::channel::<(String, BufReader<ChildStderr>)>();

    thread::spawn(move || {
        let _ = tx_out.send(read_stream(stdout, boundary));
    });
    thread::spawn(move || {
        let _ = tx_err.send(read_stream(stderr, boundary));
    });

    let deadline = Instant::now() + timeout;

    let (stdout_output, reader_out) = rx_out.recv_timeout(timeout).ok()?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(500));
    let (stderr_output, reader_err) = rx_err.recv_timeout(remaining).ok()?;

    Some((stdout_output, stderr_output, reader_out, reader_err))
}

/// Read lines from `reader` until a line containing `boundary` is found.
/// Returns the accumulated output (before the boundary) and the reader.
pub(crate) fn read_stream<R: BufRead + Send + 'static>(
    mut reader: R,
    boundary: &'static str,
) -> (String, R) {
    let mut output = String::new();
    let mut line_buffer = String::new();
    loop {
        line_buffer.clear();
        let bytes_read = reader.read_line(&mut line_buffer).unwrap_or(0);
        if bytes_read == 0 {
            break;
        }
        if line_buffer.contains(boundary) {
            let parts: Vec<&str> = line_buffer.split(boundary).collect();
            output.push_str(parts[0]);
            break;
        }
        output.push_str(&line_buffer);
    }
    (output, reader)
}

pub use manager::ExecutorManager;
pub use side_channel::{KnotMetadata, OutputMetadata, RuntimeError, RuntimeWarning, SideChannel};

/// The output produced by a successful code execution.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Plain text output (stdout).
    Text(String),
    /// A saved figure file (SVG or PNG).
    Plot(PathBuf),
    /// A saved DataFrame file.
    DataFrame(PathBuf),
    /// Both text output and a figure.
    TextAndPlot {
        /// Plain text output (stdout).
        text: String,
        /// Path to the saved figure file.
        plot: PathBuf,
    },
    /// Both a DataFrame and a figure.
    DataFrameAndPlot {
        /// Path to the saved DataFrame file.
        dataframe: PathBuf,
        /// Path to the saved figure file.
        plot: PathBuf,
    },
}

/// A named JSON dataset produced by a chunk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DataExport {
    /// Document-wide key in the Typst `knot-data` dictionary.
    pub name: String,
    /// JSON file in the execution cache (relative in persisted metadata).
    pub path: PathBuf,
}

/// Aggregated output of a successful code execution (no runtime error).
#[derive(Debug, Clone)]
pub struct ExecutionOutput {
    /// Named JSON datasets made available to Typst, independently of visible output.
    pub exports: Vec<DataExport>,
    /// The primary execution result (text, plot, DataFrame, or combination).
    pub result: ExecutionResult,
    /// Non-fatal warnings emitted during execution.
    pub warnings: Vec<RuntimeWarning>,
}

/// Outcome of a code execution attempt.
///
/// - `Ok(Success(output))` — code ran without error.
/// - `Ok(RuntimeError(error))` — code ran but raised a deterministic error
///   (cacheable; triggers Inert cascade).
/// - `Err(e)` — infrastructure failure (process crash, timeout…); not cacheable.
#[derive(Debug)]
pub enum ExecutionAttempt {
    /// Code ran without error; output is available.
    Success(ExecutionOutput),
    /// Code raised a deterministic runtime error (cacheable; triggers Inert cascade).
    RuntimeError(RuntimeError),
}

/// Graphics rendering options passed to the language executor before each chunk.
#[derive(Debug, Clone)]
pub struct GraphicsOptions {
    /// Figure width in inches.
    pub width: f64,
    /// Figure height in inches.
    pub height: f64,
    /// Resolution in dots per inch.
    pub dpi: u32,
    /// Output format string, e.g. `"svg"` or `"png"`.
    pub format: String,
}

/// Process execution output: check for errors, then convert metadata.
///
/// Shared post-execution logic for all language executors:
/// 1. Structured error from side-channel metadata (most precise) → `RuntimeError`
/// 2. Stderr fallback logging for failures not caught by the wrapper
/// 3. Successful result via `metadata_to_execution_result` → `Success`
///
/// `traceback_skip` lets each language skip its own wrapper frames from the
/// traceback (R skips 4: tryCatch/withCallingHandlers/withVisible/eval; Python: 1).
pub fn process_execution_output(
    _code: &str,
    mut metadata: side_channel::KnotMetadata,
    stdout: &str,
    stderr: &str,
    traceback_skip: usize,
) -> Result<ExecutionAttempt> {
    // Check for structured errors first (most precise)
    if let Some(mut error) = metadata.error.take() {
        // Clean up traceback by skipping internal wrapper frames
        if traceback_skip > 0 && error.traceback.len() >= traceback_skip {
            error.traceback = error.traceback.drain(traceback_skip..).collect();
        }

        log::debug!(
            "Execution failed structured: Error: {}, Call: {}",
            error.message.as_deref().unwrap_or("Unknown error"),
            error.call.as_deref().unwrap_or("unknown"),
        );

        return Ok(ExecutionAttempt::RuntimeError(error));
    }

    // Fallback: log stderr for catastrophic failures not caught by the wrapper.
    if !stderr.trim().is_empty() {
        log::debug!("Executor stderr (non-fatal): {}", stderr.trim());
    }

    Ok(ExecutionAttempt::Success(metadata_to_execution_result(
        metadata, stdout,
    )?))
}

/// Convert side-channel metadata to ExecutionOutput
///
/// This is shared logic used by all language executors (Python, R, Julia...).
/// It aggregates metadata items (text, plots, dataframes) and determines
/// the appropriate ExecutionResult variant based on what was produced.
pub fn metadata_to_execution_result(
    metadata: KnotMetadata,
    stdout_text: &str,
) -> Result<ExecutionOutput> {
    let mut text_content = String::new();
    let mut plot_path: Option<PathBuf> = None;
    let mut dataframe_path: Option<PathBuf> = None;
    let mut exports = Vec::new();
    let mut plot_count = 0usize;
    let mut dataframe_count = 0usize;
    let mut warnings = metadata.warnings;

    for item in metadata.results {
        match item {
            OutputMetadata::DataExport { name, path } => {
                exports.push(DataExport { name, path });
            }
            OutputMetadata::Text { content } => {
                if !text_content.is_empty() {
                    text_content.push('\n');
                }
                text_content.push_str(&content);
            }
            OutputMetadata::Plot { path, .. } => {
                plot_count += 1;
                plot_path = Some(path);
            }
            OutputMetadata::DataFrame { path } => {
                dataframe_count += 1;
                dataframe_path = Some(path);
            }
        }
    }

    // Until outputs are an ordered list, say what is dropped instead of
    // losing it silently.
    let mut dropped = |message: String| {
        warnings.push(RuntimeWarning {
            message,
            call: None,
            line: None,
        })
    };
    if plot_count > 1 {
        dropped(format!(
            "This chunk produced {plot_count} plots; only the last one is shown. Put each plot in its own chunk."
        ));
    }
    if dataframe_count > 1 {
        dropped(format!(
            "This chunk produced {dataframe_count} tables; only the last one is shown. Put each table in its own chunk."
        ));
    }

    if text_content.is_empty() && !stdout_text.trim().is_empty() {
        text_content = stdout_text.to_string();
    }
    if dataframe_path.is_some() && !text_content.trim().is_empty() {
        dropped(
            "Printed output is not shown because this chunk also produced a table. Print it in a separate chunk."
                .to_string(),
        );
    }

    let result = match (text_content.is_empty(), dataframe_path, plot_path) {
        (false, None, None) => ExecutionResult::Text(text_content),
        (_, Some(df), None) => ExecutionResult::DataFrame(df),
        (false, None, Some(plot)) => ExecutionResult::TextAndPlot {
            text: text_content,
            plot,
        },
        (_, None, Some(plot)) => ExecutionResult::Plot(plot),
        (_, Some(df), Some(plot)) => ExecutionResult::DataFrameAndPlot {
            dataframe: df,
            plot,
        },
        (true, None, None) => ExecutionResult::Text(String::new()),
    };

    Ok(ExecutionOutput {
        result,
        exports,
        warnings,
    })
}

/// Low-level interface for a language executor subprocess.
pub trait LanguageExecutor: Send + Sync {
    /// Spawn and initialise the executor subprocess.
    fn initialize(&mut self) -> Result<()>;
    /// Execute a code chunk and return the result or a runtime error.
    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionAttempt>;
    /// Evaluate an inline expression and return the result as a string.
    fn execute_inline(&mut self, code: &str) -> Result<String>;
    /// Execute a lightweight query and return raw stdout (no formatting)
    fn query(&mut self, code: &str) -> Result<String>;
}

/// Combined trait for language executors that support session snapshots
pub trait KnotExecutor: LanguageExecutor + Send + Sync {
    /// Save the current environment session to a file.
    fn save_session(&mut self, path: &Path) -> Result<()>;

    /// Load an environment session from a file
    fn load_session(&mut self, path: &Path) -> Result<()>;

    /// File extension for environment snapshots (.RData, .pkl, .jls)
    fn snapshot_extension(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot(name: &str) -> OutputMetadata {
        OutputMetadata::Plot {
            path: name.into(),
            format: "svg".into(),
        }
    }

    #[test]
    fn dropped_outputs_are_reported_as_warnings() {
        let metadata = KnotMetadata {
            results: vec![
                plot("a.svg"),
                plot("b.svg"),
                OutputMetadata::DataFrame {
                    path: "t.csv".into(),
                },
            ],
            ..Default::default()
        };
        let output = metadata_to_execution_result(metadata, "printed\n").unwrap();
        let messages: Vec<_> = output.warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].starts_with("This chunk produced 2 plots"));
        assert!(messages[1].starts_with("Printed output is not shown"));
        assert!(matches!(
            output.result,
            ExecutionResult::DataFrameAndPlot { .. }
        ));
    }

    #[test]
    fn single_outputs_produce_no_warning() {
        let metadata = KnotMetadata {
            results: vec![plot("a.svg")],
            ..Default::default()
        };
        let output = metadata_to_execution_result(metadata, "printed\n").unwrap();
        assert!(output.warnings.is_empty());
        assert!(matches!(output.result, ExecutionResult::TextAndPlot { .. }));
    }
}
