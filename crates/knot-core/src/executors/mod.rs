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
