//! Executor Manager - Registry pattern for multi-language support
//!
//! Manages a pool of language executors (R, Python, Julia...) using lazy initialization.
//! Executors are created on first use and cached for subsequent calls.
//!
//! # Architecture
//!
//! The manager uses a HashMap to store executor instances, indexed by language name.
//! When `get_executor(lang)` is called:
//! 1. If the executor exists in the cache, return it
//! 2. Otherwise, create a new executor, initialize it, and cache it
//!
//! This design allows:
//! - Adding new languages without modifying the Compiler
//! - Lazy initialization (only start R if document uses R)
//! - Type-safe dispatch through trait objects
//!
//! # Example
//!
//! ```no_run
//! use knot_core::executors::{ExecutorManager, GraphicsOptions, KnotExecutor};
//! use anyhow::Result;
//! use std::path::PathBuf;
//! use tempfile::TempDir;
//!
//! fn main() -> Result<()> {
//!     let temp_dir = TempDir::new().unwrap();
//!     let cache_dir = temp_dir.path().to_path_buf();
//!     let graphics = GraphicsOptions {
//!         width: 7.0, height: 5.0, dpi: 300, format: "svg".to_string(),
//!     };
//!
//!     let mut manager = ExecutorManager::new(cache_dir);
//!
//!     // First call initializes R executor
//!     let r_exec = manager.get_executor("r")?;
//!     r_exec.execute("x <- 1", &graphics)?;
//!
//!     // Second call reuses cached instance
//!     let r_exec_2 = manager.get_executor("r")?;
//!     r_exec_2.execute("print(x)", &graphics)?; // x is still in scope
//!     Ok(())
//! }

use crate::executors::{KnotExecutor, LanguageExecutor, python::PythonExecutor, r::RExecutor};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ExecutorManager {
    executors: HashMap<String, Box<dyn KnotExecutor>>,
    cache_dir: PathBuf,
}

impl ExecutorManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            executors: HashMap::new(),
            cache_dir,
        }
    }

    /// Get or initialize an executor for the given language
    pub fn get_executor(&mut self, lang: &str) -> Result<&mut (dyn KnotExecutor + '_)> {
        if !self.executors.contains_key(lang) {
            let executor: Box<dyn KnotExecutor> = match lang {
                "r" => {
                    let mut exec = RExecutor::new(self.cache_dir.clone())?;
                    exec.initialize()?;
                    Box::new(exec)
                }
                "python" => {
                    let mut exec = PythonExecutor::new(self.cache_dir.clone())?;
                    exec.initialize()?;
                    Box::new(exec)
                }
                _ => anyhow::bail!("Unsupported language: {}", lang),
            };
            self.executors.insert(lang.to_string(), executor);
        }

        match self.executors.get_mut(lang) {
            Some(executor) => Ok(executor.as_mut()),
            None => anyhow::bail!(
                "Failed to retrieve executor for language '{}' after initialization",
                lang
            ),
        }
    }

    /// Check if a language is supported
    pub fn is_supported(&self, lang: &str) -> bool {
        matches!(lang, "r" | "python")
    }

    /// Get the number of initialized executors (for testing)
    #[cfg(test)]
    pub fn executor_count(&self) -> usize {
        self.executors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::GraphicsOptions;
    use tempfile::TempDir;

    fn setup_manager() -> (TempDir, ExecutorManager) {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        let manager = ExecutorManager::new(cache_dir);
        (temp_dir, manager)
    }

    #[test]
    fn test_lazy_initialization() {
        let (_temp, manager) = setup_manager();

        // Initially no executors should be created
        assert_eq!(manager.executor_count(), 0);
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_executor_initialization() {
        let (_temp, mut manager) = setup_manager();

        // First call should create and initialize Python executor
        let result = manager.get_executor("python");
        assert!(result.is_ok());
        assert_eq!(manager.executor_count(), 1);
    }

    #[test]
    #[ignore] // Requires R installation
    fn test_r_executor_initialization() {
        let (_temp, mut manager) = setup_manager();

        // First call should create and initialize R executor
        let result = manager.get_executor("r");
        assert!(result.is_ok());
        assert_eq!(manager.executor_count(), 1);
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_executor_cached() {
        let (_temp, mut manager) = setup_manager();

        // First call should create executor
        let _exec1 = manager.get_executor("python").unwrap();
        assert_eq!(manager.executor_count(), 1);

        // Second call should reuse the same instance (not create a new one)
        let _exec2 = manager.get_executor("python").unwrap();
        assert_eq!(manager.executor_count(), 1); // Still only one executor
    }

    #[test]
    #[ignore] // Requires R and Python installation
    fn test_multiple_languages() {
        let (_temp, mut manager) = setup_manager();

        // Initialize both R and Python
        let r_exec = manager.get_executor("r");
        assert!(r_exec.is_ok());

        let py_exec = manager.get_executor("python");
        assert!(py_exec.is_ok());

        // Should have 2 executors now
        assert_eq!(manager.executor_count(), 2);
    }

    #[test]
    fn test_unsupported_language() {
        let (_temp, mut manager) = setup_manager();

        // Try to get unsupported language
        let result = manager.get_executor("julia");
        assert!(result.is_err());

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("Unsupported language"));
            assert!(error_msg.contains("julia"));
        }
    }

    #[test]
    fn test_is_supported() {
        let (_temp, manager) = setup_manager();

        // Test supported languages
        assert!(manager.is_supported("r"));
        assert!(manager.is_supported("python"));

        // Test unsupported languages
        assert!(!manager.is_supported("julia"));
        assert!(!manager.is_supported("javascript"));
        assert!(!manager.is_supported(""));
    }

    #[test]
    #[ignore] // Requires Python installation
    fn test_executor_persistence() {
        let (_temp, mut manager) = setup_manager();

        let graphics = GraphicsOptions {
            width: 7.0,
            height: 5.0,
            dpi: 300,
            format: "svg".to_string(),
        };

        // Execute code to set a variable
        let exec1 = manager.get_executor("python").unwrap();
        let result = exec1.execute("test_var = 42", &graphics);
        assert!(result.is_ok());

        // Get executor again and verify variable persists
        let exec2 = manager.get_executor("python").unwrap();
        let result = exec2.execute("print(test_var)", &graphics);
        assert!(result.is_ok());

        if let crate::executors::ExecutionResult::Text(output) = result.unwrap() {
            assert!(output.contains("42"));
        } else {
            panic!("Expected Text result");
        }
    }
}
