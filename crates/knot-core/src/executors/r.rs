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
    /// Supports:
    /// - __KNOT_SERIALIZED_CSV__ marker for dataframes
    /// - __KNOT_SERIALIZED_PLOT__ marker for plots
    fn parse_output(&self, output: &str) -> Result<ExecutionResult> {
        const CSV_MARKER: &str = "__KNOT_SERIALIZED_CSV__";
        const PLOT_MARKER: &str = "__KNOT_SERIALIZED_PLOT__";

        let csv_pos = output.find(CSV_MARKER);
        let plot_pos = output.find(PLOT_MARKER);

        match (csv_pos, plot_pos) {
            // Both markers present
            (Some(csv_start), Some(plot_start)) => {
                // Determine which comes first
                if csv_start < plot_start {
                    // CSV first, then plot
                    let csv_content = self.extract_csv_content(output, csv_start)?;
                    let csv_path = self.save_csv_to_cache(&csv_content)?;

                    let plot_path = self.extract_plot_path(output, plot_start)?;
                    let plot_cached = self.copy_plot_to_cache(&plot_path)?;

                    Ok(ExecutionResult::DataFrameAndPlot {
                        dataframe: csv_path,
                        plot: plot_cached,
                    })
                } else {
                    // Plot first, then CSV
                    let plot_path = self.extract_plot_path(output, plot_start)?;
                    let plot_cached = self.copy_plot_to_cache(&plot_path)?;

                    let csv_content = self.extract_csv_content(output, csv_start)?;
                    let csv_path = self.save_csv_to_cache(&csv_content)?;

                    Ok(ExecutionResult::DataFrameAndPlot {
                        dataframe: csv_path,
                        plot: plot_cached,
                    })
                }
            }

            // Only CSV marker
            (Some(csv_start), None) => {
                let csv_content = self.extract_csv_content(output, csv_start)?;
                let csv_path = self.save_csv_to_cache(&csv_content)?;

                // Check if there's text output before the marker
                let text_before = output[..csv_start].trim();
                if !text_before.is_empty() {
                    log::warn!("Text output before dataframe is currently ignored");
                }

                Ok(ExecutionResult::DataFrame(csv_path))
            }

            // Only plot marker
            (None, Some(plot_start)) => {
                let plot_path = self.extract_plot_path(output, plot_start)?;
                let plot_cached = self.copy_plot_to_cache(&plot_path)?;

                // Check if there's text output before the marker
                let text_before = output[..plot_start].trim();
                if !text_before.is_empty() {
                    log::warn!("Text output before plot is currently ignored");
                }

                Ok(ExecutionResult::Plot(plot_cached))
            }

            // No special markers, return as plain text
            (None, None) => Ok(ExecutionResult::Text(output.to_string())),
        }
    }

    /// Extracts CSV content after the marker
    fn extract_csv_content(&self, output: &str, marker_pos: usize) -> Result<String> {
        const CSV_MARKER: &str = "__KNOT_SERIALIZED_CSV__";
        const PLOT_MARKER: &str = "__KNOT_SERIALIZED_PLOT__";

        let content_start = marker_pos + CSV_MARKER.len();

        // Find the end: either the next marker or end of string
        let content_end = output[content_start..]
            .find(PLOT_MARKER)
            .map(|pos| content_start + pos)
            .unwrap_or(output.len());

        let csv_content = output[content_start..content_end].trim();

        if csv_content.is_empty() {
            anyhow::bail!("Empty CSV content after marker");
        }

        Ok(csv_content.to_string())
    }

    /// Extracts plot file path after the marker
    fn extract_plot_path(&self, output: &str, marker_pos: usize) -> Result<PathBuf> {
        const PLOT_MARKER: &str = "__KNOT_SERIALIZED_PLOT__";
        const CSV_MARKER: &str = "__KNOT_SERIALIZED_CSV__";

        let content_start = marker_pos + PLOT_MARKER.len();

        // Find the end: either the next marker or end of string
        let content_end = output[content_start..]
            .find(CSV_MARKER)
            .map(|pos| content_start + pos)
            .unwrap_or(output.len());

        let plot_path_str = output[content_start..content_end].trim();

        if plot_path_str.is_empty() {
            anyhow::bail!("Empty plot path after marker");
        }

        let plot_path = PathBuf::from(plot_path_str);

        if !plot_path.exists() {
            anyhow::bail!("Plot file not found: {:?}", plot_path);
        }

        Ok(plot_path)
    }

    /// Saves CSV content to cache
    fn save_csv_to_cache(&self, csv_content: &str) -> Result<PathBuf> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(csv_content.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        let filename = format!("dataframe_{}.csv", &hash[..16]);
        let csv_path = self.cache_dir.join(&filename);

        std::fs::write(&csv_path, csv_content)
            .context("Failed to write CSV to cache")?;

        Ok(csv_path)
    }

    /// Copies plot file to cache with content-based naming
    fn copy_plot_to_cache(&self, source_path: &PathBuf) -> Result<PathBuf> {
        use sha2::{Digest, Sha256};

        // Read the plot file to compute hash
        let plot_content = std::fs::read(source_path)
            .context("Failed to read plot file")?;

        let mut hasher = Sha256::new();
        hasher.update(&plot_content);
        let hash = format!("{:x}", hasher.finalize());

        // Preserve the file extension
        let extension = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("svg");

        let filename = format!("plot_{}.{}", &hash[..16], extension);
        let dest_path = self.cache_dir.join(&filename);

        // Copy the file to cache
        std::fs::copy(source_path, &dest_path)
            .context("Failed to copy plot to cache")?;

        Ok(dest_path)
    }

    /// Execute an inline R expression and return formatted result
    ///
    /// Returns either:
    /// - Plain text for scalar values (e.g., "150", "hello", "TRUE")
    /// - Backtick-wrapped text for vectors (e.g., "`[1] 1 2 3 4 5`")
    ///
    /// Fails if the result is too complex (DataFrame, Matrix, etc.)
    pub fn execute_inline(&mut self, code: &str) -> Result<String> {
        // Execute the code and get output
        let result = self.execute(code)?;

        // Extract text output
        let output = match result {
            ExecutionResult::Text(text) => text,
            ExecutionResult::DataFrame(_) => {
                anyhow::bail!("DataFrames are not supported in inline expressions. Use typst(df) in a chunk instead.")
            }
            ExecutionResult::Plot(_) => {
                anyhow::bail!("Plots are not supported in inline expressions. Use typst(gg) in a chunk instead.")
            }
            ExecutionResult::TextAndPlot { .. } | ExecutionResult::DataFrameAndPlot { .. } => {
                anyhow::bail!("Complex outputs are not supported in inline expressions.")
            }
        };

        let trimmed = output.trim();

        // Check if it's a scalar (single value with [1] prefix)
        if let Some(scalar_value) = extract_scalar_value(trimmed) {
            // Return just the value without [1] prefix
            Ok(scalar_value)
        }
        // Check if it's a short vector
        else if is_short_vector_output(trimmed) {
            // Return with backticks (code inline, no coloration)
            Ok(format!("`{}`", trimmed))
        }
        // Too complex
        else {
            anyhow::bail!(
                "Inline expression result is too complex or long.\n\
                 Result: {}\n\
                 Inline expressions should return simple scalar values or short vectors.",
                if trimmed.len() > 100 { &trimmed[..100] } else { trimmed }
            )
        }
    }
}

/// Extract scalar value from R output
/// R prints even scalars with [1] prefix, e.g., "[1] 150" or "[1] TRUE"
/// This function extracts just the value part for clean inline display
fn extract_scalar_value(s: &str) -> Option<String> {
    // Must be single line
    if s.contains('\n') {
        return None;
    }

    // Must start with [1]
    if !s.starts_with("[1]") {
        return None;
    }

    // Extract the part after [1]
    let after_prefix = s[3..].trim();

    // Check if it's a single token (scalar)
    let tokens: Vec<&str> = after_prefix.split_whitespace().collect();
    if tokens.len() != 1 {
        return None; // Multiple values = vector, not scalar
    }

    let value = tokens[0];

    // Handle quoted strings: remove quotes
    // R prints strings as [1] "Alice"
    if value.starts_with('"') && value.ends_with('"') && value.len() > 1 {
        Some(value[1..value.len() - 1].to_string())
    } else {
        Some(value.to_string())
    }
}

/// Check if R output is a short vector (starts with [1], single line, < 80 chars)
fn is_short_vector_output(s: &str) -> bool {
    // Vector characteristics:
    // - Single line
    // - Starts with [1] (R vector notation)
    // - More than one value after [1]
    // - Reasonable length (< 80 chars for inline display)

    if s.contains('\n') || !s.starts_with("[1]") || s.len() >= 80 {
        return false;
    }

    // Check if there are multiple values (not handled by extract_scalar_value)
    let after_prefix = s[3..].trim();
    let tokens: Vec<&str> = after_prefix.split_whitespace().collect();
    tokens.len() > 1
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

        // Check if stderr contains actual errors (not just warnings/messages)
        if !stderr_output.trim().is_empty() {
            let stderr_lower = stderr_output.to_lowercase();
            let is_error = stderr_lower.contains("error")
                || stderr_lower.contains("erreur")
                || stderr_lower.contains("execution arrêtée")
                || stderr_lower.contains("execution halted")
                || stderr_lower.contains("could not find function")
                || stderr_lower.contains("objet") && stderr_lower.contains("introuvable");

            if is_error {
                anyhow::bail!(
                    "R execution failed:\n\n--- Code ---\n{}\n\n--- Stderr ---\n{}\n\n--- Stdout ---\n{}",
                    code,
                    stderr_output.trim(),
                    stdout_output.trim()
                );
            } else {
                // Just warnings or informational messages, log them but don't fail
                log::warn!("R stderr (informational): {}", stderr_output.trim());
            }
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
