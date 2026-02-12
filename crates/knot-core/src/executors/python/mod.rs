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

use super::{
    ConstantObjectHandler, ExecutionResult, GraphicsOptions, KnotExecutor, LanguageExecutor,
};
use crate::parser::ChunkOptions;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use process::PythonProcess;

pub struct PythonExecutor {
    process: PythonProcess,
    cache_dir: PathBuf,
}

impl PythonExecutor {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            process: PythonProcess::uninitialized(),
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
        log::info!("✓ Loaded knot Python helper scripts");

        Ok(())
    }

    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionResult> {
        execution::execute(&mut self.process, &self.cache_dir, code, graphics)
    }

    fn execute_inline(&mut self, code: &str) -> Result<String> {
        let defaults = ChunkOptions::default_resolved();
        // Wrap in print() to get output if it's just an expression
        let wrapped_code = format!("print({})", code);
        let result = self.execute(
            &wrapped_code,
            &crate::executors::GraphicsOptions {
                width: defaults.fig_width,
                height: defaults.fig_height,
                dpi: defaults.dpi,
                format: defaults.fig_format.as_str().to_string(),
            },
        )?;

        match result {
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

use super::path_utils::escape_path_for_code;

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

impl ConstantObjectHandler for PythonExecutor {
    fn hash_object(&mut self, object_name: &str) -> Result<String> {
        // Delegate to Python helper function
        let code = format!("print(hash_object('{}'))", object_name.replace('\'', "\\'"));
        let out = self.query(&code)?;
        if out.trim() == "NONE" {
            anyhow::bail!("Object '{}' not found", object_name);
        }
        Ok(out.trim().to_string())
    }

    fn save_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let objects_dir = cache_dir.join("objects");
        std::fs::create_dir_all(&objects_dir)?;

        let object_path = objects_dir.join(format!("{}.pkl", hash));
        let path_str = escape_path_for_code(&object_path);

        // Delegate to Python helper function
        let code = format!(
            "print(save_constant('{}', '{}'))",
            object_name.replace('\'', "\\'"),
            path_str
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
        let object_path = cache_dir.join("objects").join(format!("{}.pkl", hash));

        // Verify file integrity by hashing the file (parity with R)
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

        // Delegate to Python helper function
        let code = format!(
            "print(load_constant('{}', '{}'))",
            object_name.replace('\'', "\\'"),
            path_str
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
        // Delegate to Python helper function
        let code = format!(
            "print(remove_from_env('{}'))",
            object_name.replace('\'', "\\'")
        );
        self.query(&code)?;
        log::debug!("🗑️  Removed '{}' from Python environment", object_name);
        Ok(())
    }

    fn object_extension(&self) -> &'static str {
        "pkl"
    }
}

impl PythonExecutor {
    /// Hash a file's content using xxHash64 (parity with R)
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
        let mut executor = PythonExecutor::new(cache_dir).unwrap();
        executor.initialize().unwrap();
        (temp_dir, executor)
    }

    #[test]
    fn test_python_execute_simple() {
        let (_tmp, mut executor) = setup_executor();
        let result = executor
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

        if let ExecutionResult::Text(t) = result {
            assert_eq!(t.trim(), "2");
        } else {
            panic!("Expected Text result");
        }
    }

    #[test]
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
    fn test_python_hash_object() {
        let (_tmp, mut executor) = setup_executor();
        executor
            .execute(
                "y = [1, 2, 3]",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let result = executor.hash_object("y");
        assert!(result.is_ok());
        let hash1 = result.unwrap();
        assert!(!hash1.is_empty());

        executor
            .execute(
                "y.append(4)",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        let hash2 = executor.hash_object("y").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
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
        let mut executor2 = PythonExecutor::new(tmp.path().to_path_buf()).unwrap();
        executor2.initialize().unwrap();
        executor2.load_session(&snapshot_path).unwrap();

        let result = executor2.execute_inline("z").unwrap();
        assert_eq!(result, "999");
    }
}
