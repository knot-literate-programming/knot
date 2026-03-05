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
#[derive(Debug)]
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

/// Aggregated output of a successful code execution (no runtime error).
#[derive(Debug)]
pub struct ExecutionOutput {
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
    let mut plot_count = 0usize;
    let mut dataframe_count = 0usize;

    for item in metadata.results {
        match item {
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

    if plot_count > 1 {
        log::warn!(
            "Chunk produced {} plots; only the last one is captured. \
             Knot currently supports one plot per chunk.",
            plot_count
        );
    }
    if dataframe_count > 1 {
        log::warn!(
            "Chunk produced {} dataframes; only the last one is captured. \
             Knot currently supports one dataframe per chunk.",
            dataframe_count
        );
    }

    if text_content.is_empty() && !stdout_text.trim().is_empty() {
        text_content = stdout_text.to_string();
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
        warnings: metadata.warnings,
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

/// Combined trait for language executors that support caching and constant objects
pub trait KnotExecutor: LanguageExecutor + ConstantObjectHandler + Send + Sync {
    /// Save the current environment session to a file
    fn save_session(&mut self, path: &Path) -> Result<()>;

    /// Load an environment session from a file
    fn load_session(&mut self, path: &Path) -> Result<()>;

    /// File extension for environment snapshots (.RData, .pkl, .jls)
    fn snapshot_extension(&self) -> &'static str;
}

/// Trait for managing constant objects (cache optimization)
///
/// Allows language executors to save/load large immutable objects separately
/// from environment snapshots to reduce cache size.
pub trait ConstantObjectHandler: Send + Sync {
    /// Compute the hash of an object in the language environment
    ///
    /// Uses xxHash64 for speed. Returns hex string representation.
    /// - R: Requires `digest` package
    /// - Python: Requires `xxhash` package (pip install xxhash)
    fn hash_object(&mut self, object_name: &str) -> Result<String>;

    /// Compute hashes for multiple objects in a single round-trip.
    ///
    /// Returns a map of object_name → hash (or "NONE" if not found).
    /// Default implementation calls hash_object() N times; executors
    /// should override with a batch query for better performance.
    fn hash_objects(
        &mut self,
        object_names: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::new();
        for name in object_names {
            let hash = self.hash_object(name)?;
            result.insert(name.clone(), hash);
        }
        Ok(result)
    }

    /// Save a constant object to content-addressed storage
    ///
    /// Stores the object at: cache_dir/objects/{hash}.{ext}
    fn save_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()>;

    /// Load a constant object from content-addressed storage
    ///
    /// Restores the object into the language environment
    fn load_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()>;

    /// Remove an object from the language environment
    ///
    /// Used to exclude constant objects from environment snapshots
    fn remove_from_env(&mut self, object_name: &str) -> Result<()>;

    /// File extension for serialized objects (.rds, .pkl, .jls)
    fn object_extension(&self) -> &'static str;
}
