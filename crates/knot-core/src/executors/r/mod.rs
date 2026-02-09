// R Executor - Public API and main struct
//
// This module orchestrates R code execution with caching support.
// The implementation is split across multiple submodules for clarity:
// - process: R process lifecycle management
// - execution: Code execution logic (chunks, inline, side-channel communication)
// - file_manager: Cache file operations (save CSV, copy plots)
// - formatters: Inline expression output formatting

mod execution;
mod formatters;
mod process;

use super::{
    ConstantObjectHandler, ExecutionResult, GraphicsOptions, KnotExecutor, LanguageExecutor,
};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use process::RProcess;

const BOUNDARY: &str = crate::defaults::Defaults::BOUNDARY_MARKER;

pub struct RExecutor {
    process: RProcess,
    cache_dir: PathBuf,
}

impl RExecutor {
    /// Create a new R executor
    ///
    /// # Arguments
    /// * `cache_dir` - Directory for caching R outputs
    ///
    /// The R helper script is embedded in the binary and loaded automatically
    /// during initialization.
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            process: RProcess::uninitialized(),
            cache_dir,
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

    /// Execute a lightweight R query and return raw stdout
    ///
    /// Useful for LSP features (completion, hover) where side-channel overhead is unnecessary.
    pub fn query(&mut self, code: &str) -> Result<String> {
        execution::query(&mut self.process, code)
    }
}

impl LanguageExecutor for RExecutor {
    fn initialize(&mut self) -> Result<()> {
        self.process.initialize()
    }

    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionResult> {
        execution::execute(&mut self.process, &self.cache_dir, code, graphics)
    }

    fn execute_inline(&mut self, code: &str) -> Result<String> {
        execution::execute_inline(self, code)
    }

    fn query(&mut self, code: &str) -> Result<String> {
        execution::query(&mut self.process, code)
    }
}

use super::path_utils::escape_path_for_code;

impl KnotExecutor for RExecutor {
    fn save_session(&mut self, path: &Path) -> Result<()> {
        // Delegate to R helper function
        let path_str = escape_path_for_code(path);
        let code = format!("cat(save_session('{}'))", path_str);
        let out = self.query(&code)?;
        if out.trim() == "TRUE" {
            Ok(())
        } else {
            anyhow::bail!("Failed to save R session: {}", out)
        }
    }

    fn load_session(&mut self, path: &Path) -> Result<()> {
        // Delegate to R helper function
        let path_str = escape_path_for_code(path);
        let code = format!("cat(load_session('{}'))", path_str);
        let out = self.query(&code)?;
        if out.trim() == "TRUE" {
            Ok(())
        } else {
            anyhow::bail!("Failed to load R session: {}", out)
        }
    }

    fn snapshot_extension(&self) -> &'static str {
        "RData"
    }
}

impl ConstantObjectHandler for RExecutor {
    fn hash_object(&mut self, object_name: &str) -> Result<String> {
        // Use R helper function
        let code = format!("cat(hash_object('{}'))", object_name);
        let out = self.query(&code)?;
        if out.trim() == "NONE" {
            anyhow::bail!("Object '{}' not found", object_name);
        }
        Ok(out.trim().to_string())
    }

    fn save_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let objects_dir = cache_dir.join("objects");
        std::fs::create_dir_all(&objects_dir)?;

        let object_path = objects_dir.join(format!("{}.rds", hash));
        let path_str = escape_path_for_code(&object_path);

        let code = format!(
            "cat(save_constant('{}', '{}'))",
            object_name, path_str
        );
        self.query(&code)?;

        log::debug!(
            "💾 Saved constant object '{}' to: {}",
            object_name,
            object_path.display()
        );
        Ok(())
    }

    fn load_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let object_path = cache_dir.join("objects").join(format!("{}.rds", hash));

        // Verify file integrity by hashing the file
        let actual_hash = self.hash_file(&object_path)?;
        if actual_hash != hash {
            anyhow::bail!(
                "Cache corruption detected for constant object '{}'.\n\
                 Expected hash: {}\n\
                 Actual hash: {}\n\
                 File: {}",
                object_name,
                hash,
                actual_hash,
                object_path.display()
            );
        }

        let path_str = escape_path_for_code(&object_path);
        let code = format!(
            "cat(load_constant('{}', '{}'))",
            object_name, path_str
        );
        self.query(&code)?;

        log::debug!(
            "📥 Loaded constant object '{}' from: {}",
            object_name,
            object_path.display()
        );
        Ok(())
    }

    fn remove_from_env(&mut self, object_name: &str) -> Result<()> {
        let code = format!("rm(list = '{}', envir = .GlobalEnv)", object_name);
        self.query(&code)?;
        log::debug!("🗑️  Removed '{}' from R environment", object_name);
        Ok(())
    }

    fn object_extension(&self) -> &'static str {
        "rds"
    }
}

impl RExecutor {
    /// Hash a file's content using xxHash64
    fn hash_file(&self, file_path: &Path) -> Result<String> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let hash = xxhash_rust::xxh64::xxh64(&buffer, 0);
        Ok(format!("{:x}", hash))
    }
}

impl Drop for RExecutor {
    fn drop(&mut self) {
        self.process.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_executor() -> (TempDir, RExecutor) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();

        let mut executor = RExecutor::new(cache_dir).unwrap();
        executor.initialize().unwrap();

        (temp_dir, executor)
    }

    fn default_graphics() -> super::GraphicsOptions {
        super::GraphicsOptions {
            width: 7.0,
            height: 5.0,
            dpi: 300,
            format: "svg".to_string(),
        }
    }

    #[test]
    #[ignore]
    fn test_execute_simple_expression() {
        let (_temp_dir, mut executor) = setup_executor();
        let result = executor.execute("1 + 1", &default_graphics()).unwrap();
        match result {
            ExecutionResult::Text(output) => assert!(output.contains("2")),
            _ => panic!("Expected Text result"),
        }
    }
}
