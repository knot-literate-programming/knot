// R Executor - Public API and main struct
//
// This module orchestrates R code execution with caching support.
// The implementation is split across multiple submodules for clarity:
// - process: R process lifecycle management
// - execution: Code execution logic (chunks, inline, side-channel communication)
// - file_manager: Cache file operations (save CSV, copy plots)
// - formatters: Inline expression output formatting

mod process;
mod execution;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;

    fn get_r_helper_path() -> PathBuf {
        // Get workspace root (2 levels up from knot-core crate)
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        workspace_root.join("knot-r-package/R/typst.R")
    }

    fn setup_executor() -> (TempDir, RExecutor) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        let r_helper_path = Some(get_r_helper_path());

        let mut executor = RExecutor::new(cache_dir, r_helper_path).unwrap();
        executor.initialize().unwrap();

        (temp_dir, executor)
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_simple_expression() {
        let (_temp_dir, mut executor) = setup_executor();

        let result = executor.execute("1 + 1").unwrap();

        match result {
            ExecutionResult::Text(output) => {
                assert!(output.contains("2"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_variable_assignment() {
        let (_temp_dir, mut executor) = setup_executor();

        // Assign variable
        executor.execute("x <- 42").unwrap();

        // Use variable
        let result = executor.execute("x").unwrap();

        match result {
            ExecutionResult::Text(output) => {
                assert!(output.contains("42"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_persistence_across_chunks() {
        let (_temp_dir, mut executor) = setup_executor();

        // First chunk
        executor.execute("x <- 10").unwrap();

        // Second chunk uses variable from first
        let result = executor.execute("y <- x * 2; y").unwrap();

        match result {
            ExecutionResult::Text(output) => {
                assert!(output.contains("20"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_dataframe() {
        let (_temp_dir, mut executor) = setup_executor();

        let code = r#"
df <- data.frame(a = 1:3, b = 4:6)
typst(df)
"#;

        let result = executor.execute(code).unwrap();

        match result {
            ExecutionResult::DataFrame(path) => {
                assert!(path.exists());
                assert_eq!(path.extension().unwrap(), "csv");

                // Check CSV content
                let content = std::fs::read_to_string(&path).unwrap();
                eprintln!("CSV content:\n{}", content);

                // CSV should contain column names and data
                // (Format may vary, so just check it's valid CSV with data)
                assert!(!content.is_empty());
                assert!(content.contains("a") && content.contains("b"));
                assert!(content.contains("1") && content.contains("4"));
            }
            _ => panic!("Expected DataFrame result, got {:?}", result),
        }
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_inline_scalar() {
        let (_temp_dir, mut executor) = setup_executor();

        let result = executor.execute_inline("2 + 2").unwrap();

        // Should extract just the value
        assert_eq!(result.trim(), "4");
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_inline_string() {
        let (_temp_dir, mut executor) = setup_executor();

        let result = executor.execute_inline("'hello'").unwrap();

        assert!(result.contains("hello"));
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_inline_with_variable() {
        let (_temp_dir, mut executor) = setup_executor();

        // Set up variable in chunk
        executor.execute("x <- 100").unwrap();

        // Use in inline
        let result = executor.execute_inline("x * 2").unwrap();

        assert_eq!(result.trim(), "200");
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_inline_vector_formatted() {
        let (_temp_dir, mut executor) = setup_executor();

        let result = executor.execute_inline("1:5").unwrap();

        // Vectors should be wrapped in backticks
        assert!(result.starts_with("`"));
        assert!(result.ends_with("`"));
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_inline_rejects_dataframe() {
        let (_temp_dir, mut executor) = setup_executor();

        // Try to use typst(df) in inline - should fail
        let code = "df <- data.frame(a = 1:3); typst(df)";
        let result = executor.execute_inline(code);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        eprintln!("Error message: {}", error_msg);
        assert!(error_msg.contains("DataFrames are not supported in inline"));
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_error_handling() {
        let (_temp_dir, mut executor) = setup_executor();

        // Invalid R code
        let result = executor.execute("this_function_does_not_exist()");

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("could not find function") || error_msg.contains("introuvable"));
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_multiline_code() {
        let (_temp_dir, mut executor) = setup_executor();

        let code = r#"
x <- 1
y <- 2
z <- x + y
z
"#;

        let result = executor.execute(code).unwrap();

        match result {
            ExecutionResult::Text(output) => {
                assert!(output.contains("3"));
            }
            _ => panic!("Expected Text result"),
        }
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_execute_with_comments() {
        let (_temp_dir, mut executor) = setup_executor();

        let code = r#"
# This is a comment
x <- 5  # Assign 5 to x
x * 2   # Multiply by 2
"#;

        let result = executor.execute(code).unwrap();

        match result {
            ExecutionResult::Text(output) => {
                assert!(output.contains("10"));
            }
            _ => panic!("Expected Text result"),
        }
    }
}
