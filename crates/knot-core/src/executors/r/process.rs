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
    _helper_file: Option<tempfile::NamedTempFile>,
}

impl RProcess {
    /// Create an uninitialized R process (to be initialized later)
    pub fn uninitialized() -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            stderr: None,
            _helper_file: None,
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

        // Load knot helper functions (stored in RProcess struct to keep it alive)
        // Combine all R helper files into one temp file
        let mut temp_file = tempfile::Builder::new().suffix(".R").tempfile()?;
        for (_name, content) in crate::R_HELPERS {
            writeln!(temp_file, "{}", content)?;
        }
        let temp_path = temp_file.path().to_string_lossy().replace('\\', "\\\\");

        // Source the combined temp file
        writeln!(stdin, "source(\"{}\")", temp_path)?;

        // Send boundary markers to signal end of initialization
        writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
        writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;

        stdin.flush()?;

        self.stdin = Some(stdin);
        self.stdout = child.stdout.take().map(BufReader::new);
        self.stderr = child.stderr.take().map(BufReader::new);
        self.child = Some(child);
        self._helper_file = Some(temp_file);

        // Consume the initialization output
        let _ = self.read_until_boundary()?;

        Ok(())
    }

    /// Read from stdout and stderr until boundary markers are reached
    pub fn read_until_boundary(&mut self) -> Result<(String, String)> {
        let stdout = self
            .stdout
            .as_mut()
            .context("R process stdout is not available")?;
        let stderr = self
            .stderr
            .as_mut()
            .context("R process stderr is not available")?;

        let (stdout_output, stderr_output) = thread::scope(|s| {
            let stdout_handle = s.spawn(move || {
                let mut output = String::new();
                let mut line_buffer = String::new();
                loop {
                    line_buffer.clear();
                    let bytes_read = stdout.read_line(&mut line_buffer).unwrap_or(0);
                    if bytes_read == 0 {
                        break;
                    }

                    if line_buffer.contains(BOUNDARY) {
                        // Extract everything before the boundary
                        let parts: Vec<&str> = line_buffer.split(BOUNDARY).collect();
                        output.push_str(parts[0]);
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

                    if line_buffer.contains(BOUNDARY) {
                        // Extract everything before the boundary
                        let parts: Vec<&str> = line_buffer.split(BOUNDARY).collect();
                        output.push_str(parts[0]);
                        break;
                    }
                    output.push_str(&line_buffer);
                }
                output
            });

            (stdout_handle.join().unwrap(), stderr_handle.join().unwrap())
        });

        Ok((stdout_output, stderr_output))
    }

    /// Terminate the R process
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
