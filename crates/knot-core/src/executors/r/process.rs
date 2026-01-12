// R Process Management
//
// Handles the lifecycle of the persistent R subprocess:
// - Spawning the R process
// - Loading the knot R package
// - Managing stdin/stdout/stderr streams
// - Terminating the process

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
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
    pub fn initialize(&mut self) -> Result<()> {
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

        // Load the knot R package if available
        self.load_knot_package()?;

        Ok(())
    }

    /// Attempts to load the knot R package.
    /// This is called during initialization. If the package is not installed,
    /// it logs a warning but does not fail - users can still use knot without it.
    fn load_knot_package(&mut self) -> Result<()> {
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

        // Try to load the package
        writeln!(stdin, "library(knot.r.package)")?;
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

        // If there's an error, it likely means the package is not installed
        // We don't fail here - just log a warning
        if !stderr_output.trim().is_empty() {
            log::warn!("Could not load knot.r.package: {}", stderr_output.trim());
            log::warn!("Rich output features (dataframes, plots) will not be available.");
        } else {
            log::info!("✓ Loaded knot.r.package");
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
