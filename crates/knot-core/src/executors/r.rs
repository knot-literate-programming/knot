use super::{ExecutionResult, LanguageExecutor};
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;

const BOUNDARY: &str = "---KNOT_CHUNK_BOUNDARY---";

pub struct RExecutor {
    #[allow(dead_code)]
    cache_dir: PathBuf,
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    stderr: Option<BufReader<ChildStderr>>,
}

impl RExecutor {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            cache_dir,
            process: None,
            stdin: None,
            stdout: None,
            stderr: None,
        })
    }
}

impl LanguageExecutor for RExecutor {
    /// Spawns a persistent R process.
    fn initialize(&mut self) -> Result<()> {
        let mut child = Command::new("R")
            .arg("--vanilla")
            .arg("--quiet")
            .arg("--no-save")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()) // Pipe stderr to check for errors
            .spawn()
            .context("R not found. Please install R and ensure it's in your PATH.")?;

        // Disable command echoing in the R session
        let mut stdin = child.stdin.take().context("Failed to open R stdin")?;
        writeln!(stdin, "options(echo = FALSE)")?;
        stdin.flush()?;

        self.stdin = Some(stdin);
        self.stdout = child.stdout.take().map(BufReader::new);
        self.stderr = child.stderr.take().map(BufReader::new);
        self.process = Some(child);

        Ok(())
    }

    /// Executes a code chunk in the persistent R process.
    fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
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

        // Write the code, followed by a newline and the boundary command to stdout.
        // We also write a boundary to stderr.
        writeln!(stdin, "{}", code)?;
        writeln!(stdin, "cat('{}
', file=stdout())", BOUNDARY)?;
        writeln!(stdin, "cat('{}
', file=stderr())", BOUNDARY)?;
        stdin.flush()?;

        let (stdout_output, stderr_output) = thread::scope(|s| {
            let stdout_handle = s.spawn(move || {
                let mut output = String::new();
                let mut line_buffer = String::new();
                loop {
                    line_buffer.clear();
                    // Using read_line on a BufReader is the correct approach.
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

        if !stderr_output.trim().is_empty() {
            anyhow::bail!(
                "R execution failed:\n\n--- Code ---\n{}\n\n--- Stderr ---\n{}\n\n--- Stdout ---\n{}",
                code,
                stderr_output.trim(),
                stdout_output.trim()
            );
        }

        Ok(ExecutionResult::Text(stdout_output))
    }
}

impl Drop for RExecutor {
    /// Ensures the R process is terminated when the executor is dropped.
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
        }
    }
}
