//! Python Process Management
//!
//! Manages the lifecycle of a persistent Python3 subprocess using an embedded event loop wrapper.
#![allow(missing_docs)]

use crate::executors::read_streams_until_boundary;
use anyhow::{Context, Result, anyhow};
use std::io::{BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

pub const BOUNDARY: &str = crate::defaults::Defaults::BOUNDARY_MARKER;

// The wrapper script runs an infinite loop reading commands from stdin.
const PYTHON_WRAPPER: &str = r#"
import sys
import traceback

# Force unbuffered output
sys.stdout.reconfigure(line_buffering=True)
sys.stderr.reconfigure(line_buffering=True)

def main():
    while True:
        try:
            # Read code block
            code_lines = []
            while True:
                line = sys.stdin.readline()
                if not line: return # EOF
                line = line.rstrip('\n')
                if line == "END_EXEC":
                    break
                code_lines.append(line)

            code = "\n".join(code_lines)

            try:
                # Execute code in global scope
                exec(code, globals())
            except Exception:
                # Print traceback to stderr
                traceback.print_exc(file=sys.stderr)

            # Flush streams and print boundary
            sys.stdout.write("---KNOT_CHUNK_BOUNDARY---\n")
            sys.stdout.flush()
            sys.stderr.write("---KNOT_CHUNK_BOUNDARY---\n")
            sys.stderr.flush()

        except KeyboardInterrupt:
            return
        except Exception as e:
            sys.stderr.write(f"Internal wrapper error: {e}\n")
            sys.stderr.flush()

if __name__ == "__main__":
    main()
"#;

pub struct PythonProcess {
    child: Option<Child>,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout: Option<BufReader<ChildStdout>>,
    pub(super) stderr: Option<BufReader<ChildStderr>>,
    timeout: Duration,
}

impl PythonProcess {
    /// Create an uninitialized Python process with the given execution timeout.
    pub fn uninitialized(timeout: Duration) -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            stderr: None,
            timeout,
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        let mut child = Command::new("python3")
            .arg("-u") // Unbuffered
            .arg("-c") // Execute the wrapper passed as string
            .arg(PYTHON_WRAPPER)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("python3 not found.")?;

        self.stdin = Some(child.stdin.take().context("Failed to open Python stdin")?);
        self.stdout = child.stdout.take().map(BufReader::new);
        self.stderr = child.stderr.take().map(BufReader::new);
        self.child = Some(child);

        Ok(())
    }

    pub fn execute_code(&mut self, code: &str) -> Result<()> {
        let stdin = self.stdin.as_mut().context("Stdin not available")?;

        // Write code followed by our custom delimiter
        writeln!(stdin, "{}", code)?;
        writeln!(stdin, "END_EXEC")?;
        stdin.flush()?;
        Ok(())
    }

    /// Read from stdout and stderr until boundary markers are reached.
    ///
    /// Spawns two threads to read concurrently and waits for both with a timeout.
    /// If either stream does not produce a boundary within `self.timeout`, the
    /// child process is killed and an error is returned.
    pub fn read_until_boundary(&mut self) -> Result<(String, String)> {
        let stdout = self.stdout.take().context("Python stdout not available")?;
        let stderr = self.stderr.take().context("Python stderr not available")?;

        match read_streams_until_boundary(stdout, stderr, self.timeout, BOUNDARY) {
            Some((out, err, reader_out, reader_err)) => {
                self.stdout = Some(reader_out);
                self.stderr = Some(reader_err);
                Ok((out, err))
            }
            None => {
                self.terminate();
                Err(anyhow!(
                    "Python execution timed out after {} seconds",
                    self.timeout.as_secs()
                ))
            }
        }
    }

    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
