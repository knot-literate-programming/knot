// R Process Management
//
// Handles the lifecycle of the persistent R subprocess:
// - Spawning the R process
// - Loading the knot R package
// - Managing stdin/stdout/stderr streams
// - Terminating the process

use anyhow::{Context, Result, anyhow};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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

        let (tx_out, rx_out) = mpsc::channel::<(String, BufReader<ChildStdout>)>();
        let (tx_err, rx_err) = mpsc::channel::<(String, BufReader<ChildStderr>)>();

        thread::spawn(move || {
            let _ = tx_out.send(read_stream(stdout, BOUNDARY));
        });
        thread::spawn(move || {
            let _ = tx_err.send(read_stream(stderr, BOUNDARY));
        });

        let deadline = Instant::now() + self.timeout;

        match rx_out.recv_timeout(self.timeout) {
            Ok((stdout_output, reader_out)) => {
                let remaining = deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(500));
                match rx_err.recv_timeout(remaining) {
                    Ok((stderr_output, reader_err)) => {
                        self.stdout = Some(reader_out);
                        self.stderr = Some(reader_err);
                        Ok((stdout_output, stderr_output))
                    }
                    Err(_) => {
                        self.terminate();
                        Err(anyhow!(
                            "R execution timed out after {} seconds",
                            self.timeout.as_secs()
                        ))
                    }
                }
            }
            Err(_) => {
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

/// Read lines from `reader` until a line containing `boundary` is found.
/// Returns the accumulated output (before the boundary) and the reader.
fn read_stream<R: BufRead + Send + 'static>(mut reader: R, boundary: &'static str) -> (String, R) {
    let mut output = String::new();
    let mut line_buffer = String::new();
    loop {
        line_buffer.clear();
        let bytes_read = reader.read_line(&mut line_buffer).unwrap_or(0);
        if bytes_read == 0 {
            break;
        }
        if line_buffer.contains(boundary) {
            let parts: Vec<&str> = line_buffer.split(boundary).collect();
            output.push_str(parts[0]);
            break;
        }
        output.push_str(&line_buffer);
    }
    (output, reader)
}
