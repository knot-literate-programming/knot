// R Process Management
//
// Handles the lifecycle of the persistent R subprocess:
// - Spawning the R process
// - Loading the knot R package
// - Managing stdin/stdout/stderr streams
// - Terminating the process

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;

use super::BOUNDARY;

pub struct RProcess {
    child: Option<Child>,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout: Option<BufReader<ChildStdout>>,
    pub(super) stderr: Option<BufReader<ChildStderr>>,
}

impl RProcess {
    /// Create an uninitialized R process (to be initialized later)
    pub fn uninitialized() -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    /// Initialize and spawn the R process
    pub fn initialize(&mut self, r_helper_path: Option<PathBuf>) -> Result<()> {
        let mut child = Command::new("R")
            .arg("--vanilla")
            .arg("--quiet")
            .arg("--no-save")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("R not found. Please install R and ensure it's in your PATH.")?;

        // Disable command echoing in the R session
        let mut stdin = child.stdin.take().context("Failed to open R stdin")?;
        writeln!(stdin, "options(echo = FALSE)")?;
        stdin.flush()?;

        self.stdin = Some(stdin);
        self.stdout = child.stdout.take().map(BufReader::new);
        self.stderr = child.stderr.take().map(BufReader::new);
        self.child = Some(child);

        // Load the knot R package or source the helper file
        self.load_knot_helpers(r_helper_path)?;

        Ok(())
    }

    /// Load knot R helpers (either from local file or installed package)
    ///
    /// Priority:
    /// 1. If r_helper_path is provided and exists → source("path/to/file.R")
    /// 2. Otherwise → library(knot.r.package) (fallback to installed package)
    ///
    /// This is called during initialization. If loading fails, it logs a warning
    /// but does not fail - users can still use knot without rich output features.
    fn load_knot_helpers(&mut self, r_helper_path: Option<PathBuf>) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("R process stdin is not available")?;
        let stdout = self
            .stdout
            .as_mut()
            .context("R process stdout is not available")?;
        let stderr = self
            .stderr
            .as_mut()
            .context("R process stderr is not available")?;

        // Determine loading strategy
        let load_command = if let Some(ref path) = r_helper_path {
            if path.exists() {
                // Source the local R helper file
                let path_str = path.to_string_lossy();
                log::info!("Loading R helpers from: {}", path_str);
                format!("source(\"{}\")", path_str)
            } else {
                log::warn!("R helper file not found: {}. Falling back to library(knot.r.package)", path.display());
                "library(knot.r.package)".to_string()
            }
        } else {
            // No path specified, try to load the installed package
            log::info!("No R helper path specified. Trying library(knot.r.package)");
            "library(knot.r.package)".to_string()
        };

        // Execute the load command
        writeln!(stdin, "{}", load_command)?;
        writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
        writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
        stdin.flush()?;

        // Collect output
        let (_stdout_output, stderr_output) = thread::scope(|s| {
            let stdout_handle = s.spawn(move || {
                let mut output = String::new();
                let mut line_buffer = String::new();
                loop {
                    line_buffer.clear();
                    let bytes_read = stdout.read_line(&mut line_buffer).unwrap_or(0);
                    if bytes_read == 0 {
                        break;
                    }
                    if line_buffer.trim_end() == BOUNDARY {
                        break;
                    }
                    output.push_str(&line_buffer);
                }
                output
            });

            let stderr_handle = s.spawn(move || {
                let mut output = String::new();
                let mut line_buffer = String::new();
                loop {
                    line_buffer.clear();
                    let bytes_read = stderr.read_line(&mut line_buffer).unwrap_or(0);
                    if bytes_read == 0 {
                        break;
                    }
                    if line_buffer.trim_end() == BOUNDARY {
                        break;
                    }
                    output.push_str(&line_buffer);
                }
                output
            });

            (
                stdout_handle.join().unwrap(),
                stderr_handle.join().unwrap(),
            )
        });

        // If there's an error, it likely means the package/file is not available
        // We don't fail here - just log a warning
        if !stderr_output.trim().is_empty() {
            log::warn!("Could not load R helpers: {}", stderr_output.trim());
            log::warn!("Rich output features (dataframes, plots) will not be available.");
        } else {
            if r_helper_path.is_some() {
                log::info!("✓ Loaded R helpers from local file");
            } else {
                log::info!("✓ Loaded knot.r.package");
            }
        }

        Ok(())
    }

    /// Terminate the R process
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
