use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod error_utils;
pub mod manager;
pub mod path_utils;
pub mod python;
pub mod r;
pub mod side_channel;

pub use manager::ExecutorManager;
use side_channel::{KnotMetadata, OutputMetadata, RuntimeWarning, SideChannel};

// From section 3.3 of the reference document

#[derive(Debug)]
pub enum ExecutionResult {
    Text(String),
    Plot(PathBuf),
    DataFrame(PathBuf),
    TextAndPlot { text: String, plot: PathBuf },
    DataFrameAndPlot { dataframe: PathBuf, plot: PathBuf },
}

/// Aggregated output of a code execution
#[derive(Debug)]
pub struct ExecutionOutput {
    pub result: ExecutionResult,
    pub warnings: Vec<RuntimeWarning>,
}

/// Graphics options for code execution
#[derive(Debug, Clone)]
pub struct GraphicsOptions {
    pub width: f64,
    pub height: f64,
    pub dpi: u32,
    pub format: String,
}

/// Process execution output: check for errors, then convert metadata.
///
/// Shared post-execution logic for all language executors:
/// 1. Structured error from side-channel metadata (most precise)
/// 2. Stderr fallback for errors not caught by the wrapper (e.g. syntax errors)
/// 3. Successful result via `metadata_to_execution_result`
///
/// `traceback_skip` lets each language skip its own wrapper frames from the
/// traceback (R skips 3: tryCatch/withCallingHandlers/withVisible; Python: 0).
pub fn process_execution_output(
    code: &str,
    metadata: side_channel::KnotMetadata,
    stdout: &str,
    stderr: &str,
    traceback_skip: usize,
) -> Result<ExecutionOutput> {
    use crate::executors::error_utils::format_code_with_context;

    // Check for structured errors first (most precise)
    if let Some(error) = &metadata.error {
        let error_msg = error
            .message
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "Unknown error".to_string());

        let code_preview = format_code_with_context(code, &error_msg, 3);

        const MAX_TRACEBACK_FRAMES: usize = 8;
        let user_frames: Vec<&String> = error.traceback.iter().skip(traceback_skip).collect();
        let traceback_str = if user_frames.len() > MAX_TRACEBACK_FRAMES {
            let omitted = user_frames.len() - MAX_TRACEBACK_FRAMES;
            let tail = &user_frames[user_frames.len() - MAX_TRACEBACK_FRAMES..];
            std::iter::once(format!("... {} frames omitted ...", omitted))
                .chain(tail.iter().map(|s| s.to_string()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            user_frames
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };

        anyhow::bail!(
            "Execution failed.\n\nCode:\n{}\n\nError: {}\nCall: {}\n\nTraceback:\n{}",
            code_preview,
            error_msg,
            error
                .call
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            traceback_str
        );
    }

    // Since we now use eval(parse(file=...)) in R and exec(compile(...)) in Python,
    // all syntax and runtime errors are caught by our wrappers and reported via
    // the structured metadata checked above.
    //
    // Stderr may still contain logs, messages (R), or library warnings.
    // We log these for debugging but do not treat them as fatal execution failures.
    if !stderr.trim().is_empty() {
        log::debug!("Executor stderr (non-fatal): {}", stderr.trim());
    }

    metadata_to_execution_result(metadata, stdout)
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

    for item in metadata.results {
        match item {
            OutputMetadata::Text { content } => {
                if !text_content.is_empty() {
                    text_content.push('\n');
                }
                text_content.push_str(content.as_str());
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

pub trait LanguageExecutor: Send + Sync {
    fn initialize(&mut self) -> Result<()>;
    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionOutput>;
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
