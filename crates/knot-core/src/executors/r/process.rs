// R Process Management
//
// Handles the lifecycle of the persistent R subprocess:
// - Spawning the R process
// - Loading the knot R package
// - Managing stdin/stdout/stderr streams
// - Terminating the process

use crate::executors::read_streams_until_boundary;
use anyhow::{Context, Result, anyhow};
use std::io::{BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use super::BOUNDARY;

pub struct RProcess {
    child: Option<Child>,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout: Option<BufReader<ChildStdout>>,
    pub(super) stderr: Option<BufReader<ChildStderr>>,
    timeout: Duration,
    _helper_file: Option<tempfile::NamedTempFile>,
}

impl RProcess {
    /// Create an uninitialized R process with the given execution timeout.
    pub fn uninitialized(timeout: Duration) -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            stderr: None,
            timeout,
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

    /// Read from stdout and stderr until boundary markers are reached.
    ///
    /// Spawns two threads to read concurrently and waits for both with a timeout.
    /// If either stream does not produce a boundary within `self.timeout`, the
    /// child process is killed and an error is returned.
    pub fn read_until_boundary(&mut self) -> Result<(String, String)> {
        let stdout = self
            .stdout
            .take()
            .context("R process stdout is not available")?;
        let stderr = self
            .stderr
            .take()
            .context("R process stderr is not available")?;

        match read_streams_until_boundary(stdout, stderr, self.timeout, BOUNDARY) {
            Some((out, err, reader_out, reader_err)) => {
                self.stdout = Some(reader_out);
                self.stderr = Some(reader_err);
                Ok((out, err))
            }
            None => {
                self.terminate();
                Err(anyhow!(
                    "R execution timed out after {} seconds",
                    self.timeout.as_secs()
                ))
            }
        }
    }

    /// Terminate the R process
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
