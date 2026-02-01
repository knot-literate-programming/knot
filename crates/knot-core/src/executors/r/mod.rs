// R Executor - Public API and main struct
//
// This module orchestrates R code execution with caching support.
// The implementation is split across multiple submodules for clarity:
// - process: R process lifecycle management
// - execution: Code execution logic (chunks, inline, side-effects)
// - output_parser: Parsing R output markers (CSV, Plot)
// - file_manager: Cache file operations (save CSV, copy plots)
// - formatters: Inline expression output formatting

mod process;
mod execution;
mod output_parser;
mod file_manager;
mod formatters;

use super::{ExecutionResult, LanguageExecutor};
use anyhow::Result;
use std::path::PathBuf;

pub use process::RProcess;

const BOUNDARY: &str = "---KNOT_CHUNK_BOUNDARY---";

pub struct RExecutor {
    process: RProcess,
    cache_dir: PathBuf,
    r_helper_path: Option<PathBuf>,
}

impl RExecutor {
    /// Create a new R executor
    ///
    /// # Arguments
    /// * `cache_dir` - Directory for caching R outputs
    /// * `r_helper_path` - Optional path to R helper file (e.g., "lib/knot.R")
    ///                     If None, will try to load the installed knot.r.package
    pub fn new(cache_dir: PathBuf, r_helper_path: Option<PathBuf>) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            process: RProcess::uninitialized(),
            cache_dir,
            r_helper_path,
        })
    }

    /// Execute an inline R expression and return formatted result
    ///
    /// Returns either:
    /// - Plain text for scalar values (e.g., "150", "hello", "TRUE")
    /// - Backtick-wrapped text for vectors (e.g., "`[1] 1 2 3 4 5`")
    ///
    /// Fails if the result is too complex (DataFrame, Matrix, etc.)
    pub fn execute_inline(&mut self, code: &str) -> Result<String> {
        execution::execute_inline(self, code)
    }

    /// Execute an R expression for its side effects, discarding all output.
    pub fn execute_side_effect_only(&mut self, code: &str) -> Result<()> {
        execution::execute_side_effect_only(&mut self.process, code)
    }
}

impl LanguageExecutor for RExecutor {
    fn initialize(&mut self) -> Result<()> {
        self.process.initialize(self.r_helper_path.clone())
    }

    fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        execution::execute(&mut self.process, &self.cache_dir, code)
    }
}

impl Drop for RExecutor {
    fn drop(&mut self) {
        self.process.terminate();
    }
}
