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
//! let mut executor = PythonExecutor::new(cache_dir)?;
//! executor.initialize()?;
//!
//! // Execute a chunk
//! let result = executor.execute("x = 1 + 1\nprint(x)", &graphics)?;
//!
//! // Execute inline expression
//! let value = executor.execute_inline("x * 2")?; // Returns "4"
//! ```

pub mod process;

use std::path::{Path, PathBuf};
use anyhow::Result;

use crate::executors::{
    ConstantObjectHandler, ExecutionResult, GraphicsOptions, 
    LanguageExecutor, KnotExecutor
};
use process::PythonProcess;

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
}

impl LanguageExecutor for PythonExecutor {
    fn initialize(&mut self) -> Result<()> {
        self.process.initialize()?;

        // Add knot-python-package to sys.path and import automatically
        // This allows users to use typst() and current_plot() without explicit imports
        let knot_package_path = get_knot_python_package_path()?;
        let setup_code = format!(
            r#"
import sys
sys.path.insert(0, r'{}')

# Automatically import knot functions (like R does)
# This makes typst() and current_plot() available globally
from knot import *
"#,
            knot_package_path.display()
        );

        self.query(&setup_code)?;
        log::info!("✓ Loaded knot Python package from: {}", knot_package_path.display());

        Ok(())
    }

    fn execute(&mut self, code: &str, _graphics: &GraphicsOptions) -> Result<ExecutionResult> {
        self.process.execute_code(code)?;
        let (stdout, stderr) = self.process.read_until_boundary()?;

        if !stderr.is_empty() && stderr.to_lowercase().contains("traceback") {
            anyhow::bail!("Python execution failed:\n{}", stderr);
        }

        Ok(ExecutionResult::Text(stdout))
    }

    fn execute_inline(&mut self, code: &str) -> Result<String> {
        // Wrap in print() to get output if it's just an expression
        let wrapped_code = format!("print({})", code);
        let result = self.execute(&wrapped_code, &crate::executors::GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        })?;

        match result {
            ExecutionResult::Text(t) => Ok(t.trim().to_string()),
            _ => anyhow::bail!("Unexpected result type for inline expression"),
        }
    }

    fn query(&mut self, code: &str) -> Result<String> {
        let result = self.execute(code, &crate::executors::GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        })?;
        match result {
            ExecutionResult::Text(t) => Ok(t.trim().to_string()),
            _ => anyhow::bail!("Unexpected result type for query"),
        }
    }
}

impl KnotExecutor for PythonExecutor {
    fn save_session(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().replace('\\', "\\\\");
        let code = format!(
            "import pickle\n\
             import types\n\
             state = {{}}\n\
             for k, v in list(globals().items()):\n    \
                 if k.startswith('__') or isinstance(v, types.ModuleType):\n        \
                     continue\n    \
                 try:\n        \
                     # Test if picklable\n        \
                     pickle.dumps(v)\n        \
                     state[k] = v\n    \
                 except:\n        \
                     pass\n\
             with open('{}', 'wb') as f:\n    \
                 pickle.dump(state, f)",
            path_str
        );
        self.query(&code)?;
        Ok(())
    }

    fn load_session(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().replace('\\', "\\\\");
        let code = format!(
            "import pickle\n\
             with open('{}', 'rb') as f:\n    \
                 state = pickle.load(f)\n    \
                 globals().update(state)",
            path_str
        );
        self.query(&code)?;
        Ok(())
    }

    fn snapshot_extension(&self) -> &'static str {
        "pkl"
    }
}

impl ConstantObjectHandler for PythonExecutor {
    fn hash_object(&mut self, object_name: &str) -> Result<String> {
        let code = format!(
            "try:\n    \
                 import xxhash\n\
             except ImportError:\n    \
                 raise ImportError('Package xxhash is required for constant objects. Install with: pip install xxhash')\n\
             import pickle\n\
             obj = globals().get('{}')\n\
             if obj is None:\n    \
                 print('NONE')\n\
             else:\n    \
                 h = xxhash.xxh64(pickle.dumps(obj)).hexdigest()\n    \
                 print(h)",
            object_name
        );
        let out = self.query(&code)?;
        if out.trim() == "NONE" {
            anyhow::bail!("Object not found");
        }
        Ok(out.trim().to_string())
    }

    fn save_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let obj_path = cache_dir.join("objects").join(format!("{}.pkl", hash));
        let path_str = obj_path.to_string_lossy().replace('\\', "\\\\");
        let code = format!(
            "import pickle\n\
             with open('{}', 'wb') as f:\n    \
                 pickle.dump(globals()['{}'], f)",
            path_str, object_name
        );
        self.query(&code)?;
        Ok(())
    }

    fn load_constant(&mut self, object_name: &str, hash: &str, cache_dir: &Path) -> Result<()> {
        let obj_path = cache_dir.join("objects").join(format!("{}.pkl", hash));
        let path_str = obj_path.to_string_lossy().replace('\\', "\\\\");
        let code = format!(
            "import pickle\n\
             with open('{}', 'rb') as f:\n    \
                 globals()['{}'] = pickle.load(f)",
            path_str, object_name
        );
        self.query(&code)?;
        Ok(())
    }

    fn remove_from_env(&mut self, object_name: &str) -> Result<()> {
        let code = format!("del globals()['{}']", object_name);
        self.query(&code)?;
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

/// Get the path to the knot-python-package directory
///
/// Searches in the following locations (in order):
/// 1. KNOT_PYTHON_PACKAGE_PATH environment variable
/// 2. Relative to the current executable (../../knot-python-package)
/// 3. Relative to workspace root
fn get_knot_python_package_path() -> Result<PathBuf> {
    // 1. Check environment variable
    if let Ok(env_path) = std::env::var("KNOT_PYTHON_PACKAGE_PATH") {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return Ok(path);
        }
        log::warn!(
            "KNOT_PYTHON_PACKAGE_PATH set but path does not exist: {}",
            path.display()
        );
    }

    // 2. Try relative to current executable
    if let Ok(exe_path) = std::env::current_exe() {
        // Assuming structure: target/debug/knot or target/release/knot
        // Go up to workspace root: ../../
        if let Some(target_dir) = exe_path.parent() {
            if let Some(workspace_root) = target_dir.parent() {
                let package_path = workspace_root.join("knot-python-package");
                if package_path.exists() {
                    return Ok(package_path);
                }
            }
        }
    }

    // 3. Try relative to current directory (for development)
    let cwd_package = PathBuf::from("knot-python-package");
    if cwd_package.exists() {
        return Ok(cwd_package);
    }

    // 4. Try workspace root (from CARGO_MANIFEST_DIR at compile time)
    #[cfg(debug_assertions)]
    {
        // In debug mode, try to use compile-time manifest dir
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(workspace_root) = manifest_dir.parent() {
            if let Some(workspace_root) = workspace_root.parent() {
                let package_path = workspace_root.join("knot-python-package");
                if package_path.exists() {
                    return Ok(package_path);
                }
            }
        }
    }

    anyhow::bail!(
        "Could not find knot-python-package directory.\n\
         Searched in:\n\
         - KNOT_PYTHON_PACKAGE_PATH environment variable\n\
         - Relative to executable\n\
         - Current directory\n\
         \n\
         You can set KNOT_PYTHON_PACKAGE_PATH to specify the location."
    )
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
        let result = executor.execute("print(1 + 1)", &GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        }).unwrap();

        if let ExecutionResult::Text(t) = result {
            assert_eq!(t.trim(), "2");
        } else {
            panic!("Expected Text result");
        }
    }

    #[test]
    fn test_python_persistence() {
        let (_tmp, mut executor) = setup_executor();
        executor.execute("x = 100", &GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        }).unwrap();

        let result = executor.execute_inline("x").unwrap();
        assert_eq!(result, "100");
    }

    #[test]
    fn test_python_hash_object() {
        let (_tmp, mut executor) = setup_executor();
        executor.execute("y = [1, 2, 3]", &GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        }).unwrap();

        let hash1 = executor.hash_object("y").unwrap();
        assert!(!hash1.is_empty());

        executor.execute("y.append(4)", &GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        }).unwrap();

        let hash2 = executor.hash_object("y").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_python_save_load_session() {
        let (tmp, mut executor) = setup_executor();
        let snapshot_path = tmp.path().join("session.pkl");

        executor.execute("z = 'hello'", &GraphicsOptions {
            width: 0.0, height: 0.0, dpi: 0, format: String::new()
        }).unwrap();

        executor.save_session(&snapshot_path).unwrap();

        // Create new executor and load session
        let mut executor2 = PythonExecutor::new(tmp.path().to_path_buf()).unwrap();
        executor2.initialize().unwrap();
        executor2.load_session(&snapshot_path).unwrap();

        let result = executor2.execute_inline("z").unwrap();
        assert_eq!(result, "hello");
    }
}