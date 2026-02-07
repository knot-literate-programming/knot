//! Python Process Management
//!
//! Manages the lifecycle of a persistent Python3 subprocess using an embedded event loop wrapper.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;

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
}

impl PythonProcess {
    pub fn uninitialized() -> Self {
        Self {
            child: None,
            stdin: None,
            stdout: None,
            stderr: None,
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

    pub fn read_until_boundary(&mut self) -> Result<(String, String)> {
        let stdout = self
            .stdout
            .as_mut()
            .context("Python stdout not available")?;
        let stderr = self
            .stderr
            .as_mut()
            .context("Python stderr not available")?;

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

    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}
