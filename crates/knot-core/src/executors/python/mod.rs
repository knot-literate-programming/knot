//! Python Executor
//!
//! Manages a persistent Python3 subprocess for executing code chunks and inline expressions.
//!
//! # Architecture
//!
//! The executor maintains a single Python process that runs for the entire document compilation.
//! Code is executed in a shared global namespace, allowing variables to persist across chunks.
//!
//! ## Process Management
//!
//! Uses an embedded event loop wrapper (see `process.rs`) that:
//! - Runs in an infinite loop reading commands from stdin
//! - Executes code blocks using `exec()` in global scope
//! - Returns results via stdout/stderr with boundary markers
//!
//! ## Session Persistence
//!
//! Sessions are saved using Python's `pickle` module:
//! - Filters out non-picklable objects (modules, functions)
//! - Stores only user-defined variables
//! - Can restore state between compilation runs
//!
//! ## Constant Objects
//!
//! Large immutable objects can be cached separately using content-addressed storage:
//! - Objects are hashed with xxHash64 (requires `xxhash` package)
//! - Stored as `.pkl` files indexed by hash
//! - Automatically verified on load to detect corruption
//!
//! # Example
//!
//! ```rust
//! use knot_core::executors::python::PythonExecutor;
//! use knot_core::executors::{GraphicsOptions, KnotExecutor, LanguageExecutor};
//! use anyhow::Result;
//! use std::path::PathBuf;
//! use tempfile::TempDir;
//!
//! fn main() -> Result<()> {
//!     let temp_dir = TempDir::new().unwrap();
//!     let cache_dir = temp_dir.path().to_path_buf();
//!     let graphics = GraphicsOptions {
//!         width: 0.0, height: 0.0, dpi: 0, format: String::new(),
//!     };
//!
//!     let mut executor = PythonExecutor::new(cache_dir)?;
//!     executor.initialize()?;
//!
//!     // Execute a chunk
//!     let result = executor.execute("x = 1 + 1\nprint(x)", &graphics)?;
//!
//!     // Execute inline expression
//!     let value = executor.execute_inline("x * 2")?; // Returns "4"
//!     assert_eq!(value, "4");
//!     Ok(())
//! }
//! ```

pub mod process;

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::executors::{
    ConstantObjectHandler, ExecutionResult, GraphicsOptions, KnotExecutor, LanguageExecutor,
    OutputMetadata, SideChannel,
};
use process::PythonProcess;

pub struct PythonExecutor {
    process: PythonProcess,
    #[allow(dead_code)]
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
}

impl LanguageExecutor for PythonExecutor {
    fn initialize(&mut self) -> Result<()> {
        self.process.initialize()?;

        // Execute the helper script to define functions like typst() in the global scope
        self.query(crate::PYTHON_HELPER_SCRIPT)?;
        log::info!("✓ Loaded knot Python helper script");

        Ok(())
    }

    fn execute(&mut self, code: &str, graphics: &GraphicsOptions) -> Result<ExecutionResult> {
        // Create side-channel for this chunk
        let channel = SideChannel::new()?;
        channel.setup_env()?;

        // Set environment variables in the Python process
        let meta_file = escape_path_for_code(channel.path());
        let cache_dir_str = escape_path_for_code(&self.cache_dir);

        let setup_code = format!(
            "import os\n\
             os.environ['KNOT_METADATA_FILE'] = '{}'\n\
             os.environ['KNOT_CACHE_DIR'] = '{}'\n\
             os.environ['KNOT_FIG_WIDTH'] = '{}'\n\
             os.environ['KNOT_FIG_HEIGHT'] = '{}'\n\
             os.environ['KNOT_FIG_DPI'] = '{}'\n\
             os.environ['KNOT_FIG_FORMAT'] = '{}'",
            meta_file,
            cache_dir_str,
            graphics.width,
            graphics.height,
            graphics.dpi,
            graphics.format
        );

        self.process.execute_code(&setup_code)?;
        let _ = self.process.read_until_boundary()?;

        // Execute the actual code
        self.process.execute_code(code)?;
        let (stdout, stderr) = self.process.read_until_boundary()?;

        if !stderr.is_empty() && stderr.to_lowercase().contains("traceback") {
            let code_preview = if code.lines().count() > 5 {
                let lines: Vec<&str> = code.lines().take(5).collect();
                format!(
                    "{}\n... ({} lines truncated)",
                    lines.join("\n"),
                    code.lines().count() - 5
                )
            } else {
                code.to_string()
            };

            anyhow::bail!(
                "Python execution failed.\n\nCode:\n{}\n\nError:\n{}",
                code_preview,
                stderr.trim()
            );
        }

        // Read metadata from side-channel
        let metadata = channel.read_metadata()?;
        self.metadata_to_execution_result(metadata, &stdout)
    }

    fn execute_inline(&mut self, code: &str) -> Result<String> {
        // Wrap in print() to get output if it's just an expression
        let wrapped_code = format!("print({})", code);
        let result = self.execute(
            &wrapped_code,
            &crate::executors::GraphicsOptions {
                width: 0.0,
                height: 0.0,
                dpi: 0,
                format: String::new(),
            },
        )?;

        match result {
            ExecutionResult::Text(t) => Ok(t.trim().to_string()),
            _ => anyhow::bail!(
                "Inline expression returned a complex object (plot or dataframe).\n\
                 Inline code must return text or a simple value.\n\
                 Use a code chunk instead: ```{{python}}\n...\n```"
            ),
        }
    }

    fn query(&mut self, code: &str) -> Result<String> {
        let result = self.execute(
            code,
            &crate::executors::GraphicsOptions {
                width: 0.0,
                height: 0.0,
                dpi: 0,
                format: String::new(),
            },
        )?;
        match result {
            ExecutionResult::Text(t) => Ok(t.trim().to_string()),
            _ => anyhow::bail!("Internal Error: Query returned unexpected non-text result"),
        }
    }
}

impl PythonExecutor {
    /// Convert side-channel metadata to ExecutionResult
    fn metadata_to_execution_result(
        &self,
        metadata: Vec<OutputMetadata>,
        stdout_text: &str,
    ) -> Result<ExecutionResult> {
        let mut text_content = String::new();
        let mut plot_path: Option<PathBuf> = None;
        let mut dataframe_path: Option<PathBuf> = None;

        for item in metadata {
            match item {
                OutputMetadata::Text { content } => {
                    if !text_content.is_empty() {
                        text_content.push('\n');
                    }
                    text_content.push_str(&content);
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

        match (text_content.is_empty(), dataframe_path, plot_path) {
            (false, None, None) => Ok(ExecutionResult::Text(text_content)),
            (_, Some(df), None) => Ok(ExecutionResult::DataFrame(df)),
            (false, None, Some(plot)) => Ok(ExecutionResult::TextAndPlot {
                text: text_content,
                plot,
            }),
            (_, None, Some(plot)) => Ok(ExecutionResult::Plot(plot)),
            (_, Some(df), Some(plot)) => Ok(ExecutionResult::DataFrameAndPlot {
                dataframe: df,
                plot,
            }),
            (true, None, None) => Ok(ExecutionResult::Text(String::new())),
        }
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
        // Use environment variable to avoid code injection in the Python script
        unsafe {
            std::env::set_var("KNOT_OBJECT_NAME", object_name);
        }

        let code = r#"
import os
import pickle
import hashlib
try:
    import xxhash
    obj = globals().get(os.environ["KNOT_OBJECT_NAME"])
    if obj is None:
        print('NONE')
    else:
        h = xxhash.xxh64(pickle.dumps(obj)).hexdigest()
        print(h)
except ImportError:
    # Use fallback if xxhash is missing
    obj = globals().get(os.environ["KNOT_OBJECT_NAME"])
    if obj is None:
        print('NONE')
    else:
        h = hashlib.sha256(pickle.dumps(obj)).hexdigest()
        print(h)
except Exception as e:
    print(f'ERROR: {e}')
"#;
        let out = self.query(code)?;
        if out.trim() == "NONE" {
            anyhow::bail!("Object not found");
        }
        if out.trim().starts_with("ERROR:") {
            anyhow::bail!("Python error during hashing: {}", out.trim());
        }
        Ok(out.trim().to_string())
    }

    fn save_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let obj_path = cache_dir.join("objects").join(format!("{}.pkl", hash));
        let path_str = escape_path_for_code(&obj_path);

        unsafe {
            std::env::set_var("KNOT_OBJECT_NAME", object_name);
        }

        let code = format!(
            r#"
import pickle
import os
with open('{}', 'wb') as f:
    pickle.dump(globals()[os.environ['KNOT_OBJECT_NAME']], f)
"#,
            path_str
        );
        self.query(&code)?;
        Ok(())
    }

    fn load_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let obj_path = cache_dir.join("objects").join(format!("{}.pkl", hash));
        let path_str = escape_path_for_code(&obj_path);

        unsafe {
            std::env::set_var("KNOT_OBJECT_NAME", object_name);
        }

        let code = format!(
            r#"
import pickle
import os
with open('{}', 'rb') as f:
    globals()[os.environ['KNOT_OBJECT_NAME']] = pickle.load(f)
"#,
            path_str
        );
        self.query(&code)?;
        Ok(())
    }

    fn remove_from_env(&mut self, object_name: &str) -> Result<()> {
        unsafe {
            std::env::set_var("KNOT_OBJECT_NAME", object_name);
        }

        let code = "import os\ndel globals()[os.environ['KNOT_OBJECT_NAME']]";
        self.query(code)?;
        Ok(())
    }

    fn object_extension(&self) -> &'static str {
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
                "z = 'hello'",
                &GraphicsOptions {
                    width: 0.0,
                    height: 0.0,
                    dpi: 0,
                    format: String::new(),
                },
            )
            .unwrap();

        executor.save_session(&snapshot_path).unwrap();

        // Create new executor and load session
        let mut executor2 = PythonExecutor::new(tmp.path().to_path_buf()).unwrap();
        executor2.initialize().unwrap();
        executor2.load_session(&snapshot_path).unwrap();

        let result = executor2.execute_inline("z").unwrap();
        assert_eq!(result, "hello");
    }
}
