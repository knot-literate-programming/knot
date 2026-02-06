//! Python Process Management
//!
//! Manages the lifecycle of a persistent Python3 subprocess using an embedded event loop wrapper.
//!
//! # Design
//!
//! Unlike traditional approaches that spawn Python per-chunk, this implementation:
//! - Embeds a Python event loop as a string constant (`PYTHON_WRAPPER`)
//! - Spawns Python once with `-c` flag to execute the wrapper
//! - Communicates via stdin/stdout using a simple protocol
//!
//! # Protocol
//!
//! 1. **Send code**: Write lines to stdin, terminated by "END_EXEC"
//! 2. **Execute**: Python runs `exec(code, globals())`
//! 3. **Receive**: Read stdout/stderr until "---KNOT_BOUNDARY---"
//! 4. **Repeat**: Loop back to step 1
//!
//! # Thread Safety
//!
//! The `read_until_boundary()` method uses `thread::scope` to read stdout and stderr
//! concurrently, preventing deadlocks when both streams have data.
//!
//! # Example
//!
//! ```rust
//! use knot_core::executors::python::process::PythonProcess;
//! use anyhow::Result;
//!
//! fn main() -> Result<()> {
//!     let mut process = PythonProcess::uninitialized();
//!     process.initialize()?;
//!
//!     process.execute_code("x = 42\nprint(x)")?;
//!     let (stdout, stderr) = process.read_until_boundary()?;
//!     assert_eq!(stdout.trim(), "42");
//!
//!     // Variables persist across executions
//!     process.execute_code("print(x * 2)")?;
//!     let (stdout, _) = process.read_until_boundary()?;
//!     assert_eq!(stdout.trim(), "84");
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;

pub const BOUNDARY: &str = "---KNOT_BOUNDARY---";

// The wrapper script runs an infinite loop reading commands from stdin.
// It uses a simple protocol:
// 1. Read lines until "END_EXEC"
// 2. Execute the accumulated code
// 3. Print the BOUNDARY
// 4. Repeat
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
            sys.stdout.write("---KNOT_BOUNDARY---\n")
            sys.stdout.flush()
            sys.stderr.write("---KNOT_BOUNDARY---\n")
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
        let stdout = self.stdout.as_mut().context("Python stdout not available")?;
        let stderr = self.stderr.as_mut().context("Python stderr not available")?;

        let (stdout_output, stderr_output) = thread::scope(|s| {
            let stdout_handle = s.spawn(move || {
                let mut output = String::new();
                let mut line_buffer = String::new();
                loop {
                    line_buffer.clear();
                    let bytes_read = stdout.read_line(&mut line_buffer).unwrap_or(0);
                    if bytes_read == 0 { break; }
                    
                    if line_buffer.trim() == BOUNDARY { break; }
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
                    if bytes_read == 0 { break; }
                    
                    if line_buffer.trim() == BOUNDARY { break; }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_process_simple_execution() {
        let mut process = PythonProcess::uninitialized();
        process.initialize().unwrap();

        process.execute_code("print('hello')").unwrap();

        let (stdout, _) = process.read_until_boundary().unwrap();
        assert_eq!(stdout.trim(), "hello");
        
        process.terminate();
    }

    #[test]
    fn test_python_process_persistence() {
        let mut process = PythonProcess::uninitialized();
        process.initialize().unwrap();

        process.execute_code("x = 42").unwrap();
        let _ = process.read_until_boundary().unwrap();

        process.execute_code("print(x)").unwrap();
        let (stdout, _) = process.read_until_boundary().unwrap();
        assert_eq!(stdout.trim(), "42");
        
        process.terminate();
    }
}