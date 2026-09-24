#![allow(missing_docs)]
// Python Executor - Public API and main struct
//
// This module orchestrates Python code execution with caching support.
// The implementation is split across multiple submodules for clarity:
// - process: Python process lifecycle management
// - execution: Code execution logic (chunks, inline, side-channel communication)
// - formatters: Inline expression output formatting

mod execution;
mod formatters;
mod process;

use super::{ExecutionAttempt, ExecutionResult, GraphicsOptions, KnotExecutor, LanguageExecutor};
use crate::parser::ChunkOptions;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use process::PythonProcess;

pub struct PythonExecutor {
    process: PythonProcess,
    cache_dir: PathBuf,
}

impl PythonExecutor {
    /// Create a new Python executor with an execution timeout.
    ///
    /// # Arguments
    /// * `cache_dir` - Directory for caching Python outputs
    /// * `timeout`   - Maximum allowed duration for a single chunk execution
    pub fn new(cache_dir: PathBuf, timeout: Duration) -> Result<Self> {
        Self::with_executable(cache_dir, timeout, None)
    }

    /// Create an executor with an explicit interpreter path or command name.
    pub fn with_executable(
        cache_dir: PathBuf,
        timeout: Duration,
        executable: Option<PathBuf>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            process: PythonProcess::uninitialized(timeout).with_executable(executable),
            cache_dir,
        })
    }

    /// Execute a lightweight Python query and return raw stdout
    pub fn query(&mut self, code: &str) -> Result<String> {
        self.process.execute_code(code)?;
        let (stdout, _) = self.process.read_until_boundary()?;
        Ok(stdout)
    }
}

impl LanguageExecutor for PythonExecutor {
    fn initialize(&mut self) -> Result<()> {
        self.process.initialize()?;

        // Execute all helper scripts to define functions like typst() in the global scope
        for (_name, content) in crate::PYTHON_HELPERS {
            self.query(content)?;
        }
        self.query("_knot_initial_bindings = globals().copy()")?;
        log::info!("✓ Loaded knot Python helper scripts");

        Ok(())
    }

    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionAttempt> {
        execution::execute(&mut self.process, &self.cache_dir, code, graphics)
    }

    fn execute_inline(&mut self, code: &str) -> Result<String> {
        let defaults = ChunkOptions::default_resolved();
        // Wrap in print() to get output if it's just an expression
        let wrapped_code = format!("print({})", code);
        let output = self.execute(
            &wrapped_code,
            &crate::executors::GraphicsOptions {
                width: defaults.fig_width,
                height: defaults.fig_height,
                dpi: defaults.dpi,
                format: defaults.fig_format.as_str().to_string(),
            },
        )?;

        let success = match output {
            ExecutionAttempt::RuntimeError(e) => anyhow::bail!("{}", e),
            ExecutionAttempt::Success(o) => o,
        };
        match success.result {
            ExecutionResult::Text(t) => formatters::format_inline_output(&t),
            _ => anyhow::bail!(
                "Inline expression returned a complex object (plot or dataframe).\n\
                 Inline code must return text or a simple value.\n\
                 Use a code chunk instead: ```{{python}}\n...\n```"
            ),
        }
    }

    fn query(&mut self, code: &str) -> Result<String> {
        self.query(code)
    }
}

use crate::path_utils::escape_path_for_code;

impl KnotExecutor for PythonExecutor {
    fn save_session(&mut self, path: &Path) -> Result<()> {
        let path_str = escape_path_for_code(path);
        let code = format!("print(save_session('{}'))", path_str);
        let out = self.query(&code)?;
        if out.trim() == "True" {
            Ok(())
        } else {
            anyhow::bail!("Failed to save python session snapshot: {}", out)
        }
    }

    fn load_session(&mut self, path: &Path) -> Result<()> {
        let path_str = escape_path_for_code(path);
        let code = format!("print(load_session('{}'))", path_str);
        let out = self.query(&code)?;
        if out.trim() == "True" {
            Ok(())
        } else {
            anyhow::bail!("Failed to load python session snapshot: {}", out)
        }
    }

    fn snapshot_extension(&self) -> &'static str {
        "pkl"
    }
}

impl Drop for PythonExecutor {
    fn drop(&mut self) {
        self.process.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_executor() -> (TempDir, PythonExecutor) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        let mut executor =
            PythonExecutor::new(cache_dir, std::time::Duration::from_secs(30)).unwrap();
        executor.initialize().unwrap();
        (temp_dir, executor)
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_execute_simple() {
        let (_tmp, mut executor) = setup_executor();
        let output = executor
            .execute(
                "print(1 + 1)",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let success = match output {
            ExecutionAttempt::Success(o) => o,
            ExecutionAttempt::RuntimeError(e) => panic!("Expected Success, got error: {}", e),
        };
        if let ExecutionResult::Text(t) = success.result {
            assert_eq!(t.trim(), "2");
        } else {
            panic!("Expected Text result");
        }
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_persistence() {
        let (_tmp, mut executor) = setup_executor();
        executor
            .execute(
                "x = 100",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let result = executor.execute_inline("x").unwrap();
        assert_eq!(result, "100");
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_save_load_session() {
        let (tmp, mut executor) = setup_executor();
        let snapshot_path = tmp.path().join("session.pkl");

        executor
            .execute(
                "z = 999",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        executor.save_session(&snapshot_path).unwrap();

        // New executor
        let mut executor2 =
            PythonExecutor::new(tmp.path().to_path_buf(), std::time::Duration::from_secs(30))
                .unwrap();
        executor2.initialize().unwrap();
        executor2.load_session(&snapshot_path).unwrap();

        let result = executor2.execute_inline("z").unwrap();
        assert_eq!(result, "999");
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_error_handling() {
        let (_tmp, mut executor) = setup_executor();
        let output = executor
            .execute(
                "raise ValueError('Something went wrong')",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let err = match output {
            ExecutionAttempt::RuntimeError(e) => e,
            ExecutionAttempt::Success(_) => panic!("Expected RuntimeError, got Success"),
        };
        let err_msg = err.detailed_message();
        assert!(err_msg.contains("Something went wrong"));
        assert!(err_msg.contains("ValueError"));
        assert!(err_msg.contains("Traceback"));
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_warnings() {
        let (_tmp, mut executor) = setup_executor();
        let output = executor
            .execute(
                "import warnings; warnings.warn('A little warning')",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let success = match output {
            ExecutionAttempt::Success(o) => o,
            ExecutionAttempt::RuntimeError(e) => panic!("Expected Success, got error: {}", e),
        };
        assert_eq!(success.warnings.len(), 1);
        assert!(
            success.warnings[0]
                .message
                .to_string()
                .contains("A little warning")
        );
    }
}
