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

impl RExecutor {
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

    /// Parses the output from R to detect serialized data.
    /// Currently supports __KNOT_SERIALIZED_CSV__ marker for dataframes.
    fn parse_output(&self, output: &str) -> Result<ExecutionResult> {
        const CSV_MARKER: &str = "__KNOT_SERIALIZED_CSV__";

        if let Some(csv_start) = output.find(CSV_MARKER) {
            // Extract the CSV content after the marker
            let csv_content = &output[csv_start + CSV_MARKER.len()..];
            let csv_content = csv_content.trim();

            if csv_content.is_empty() {
                return Ok(ExecutionResult::Text(output.to_string()));
            }

            // Generate a unique filename based on timestamp and content hash
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(csv_content.as_bytes());
            let hash = format!("{:x}", hasher.finalize());
            let filename = format!("dataframe_{}.csv", &hash[..16]);
            let csv_path = self.cache_dir.join(&filename);

            // Save CSV to cache
            std::fs::write(&csv_path, csv_content)
                .context("Failed to write CSV to cache")?;

            // Return the rest of the output as text if there's any before the marker
            let text_before = output[..csv_start].trim();
            if !text_before.is_empty() {
                // TODO: Support Both { text, dataframe } in the future
                log::warn!("Text output before dataframe is currently ignored");
            }

            Ok(ExecutionResult::DataFrame(csv_path))
        } else {
            // No special markers, return as plain text
            Ok(ExecutionResult::Text(output.to_string()))
        }
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

        // Load the knot R package if available
        self.load_knot_package()?;

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

        // Check if output contains serialized data from knot.r.package
        self.parse_output(&stdout_output)
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
