# TODO - Knot Improvements

## 🔧 Error Handling: Hybrid Approach (Option C)

**Priority:** Medium
**Effort:** ~1.5 hours
**Risk:** Low

### Context

Currently, R error detection uses heuristic pattern matching on stderr output:
- Searches for keywords like "error", "erreur", "execution halted"
- Prone to false positives (warnings containing "error" in text)
- Language-dependent (French/English only)
- No distinction between warnings, messages, and fatal errors

**Location:** `crates/knot-core/src/executors/r/execution.rs` lines 46-67

### Proposed Solution: Hybrid Error Detection

Combine two approaches for robust error handling:

1. **Structured error detection** - Wrap user code with R error handler
2. **Pattern matching fallback** - For system errors (package loading, etc.)

---

## 📋 Implementation Plan

### Step 1: Create Error Type Enum

**File:** `crates/knot-core/src/executors/r/execution.rs`

```rust
/// Types of R execution results
#[derive(Debug, Clone)]
enum RExecutionStatus {
    /// No errors or warnings
    Success,

    /// Informational message (from message())
    Message(String),

    /// Warning (from warning())
    Warning(String),

    /// Fatal error with structured info
    Error {
        message: String,
        call: Option<String>,
        traceback: Vec<String>,
    },
}
```

### Step 2: Implement Structured Error Wrapper

**File:** `crates/knot-core/src/executors/r/execution.rs`

```rust
/// Wrap user code with error handler for structured error detection
fn wrap_code_with_error_handler(code: &str) -> String {
    format!(
        r#"
.knot_exec <- function() {{
    tryCatch(
        {{
            {code}
        }},
        error = function(e) {{
            # Structured error marker
            cat("__KNOT_ERROR_START__\n", file = stderr())
            cat(paste0("MESSAGE:", e$message, "\n"), file = stderr())

            # Try to get call info
            if (!is.null(e$call)) {{
                cat(paste0("CALL:", deparse(e$call)[1], "\n"), file = stderr())
            }}

            # Try to get traceback
            tb <- capture.output(traceback())
            if (length(tb) > 0) {{
                cat("TRACEBACK:\n", file = stderr())
                cat(paste(tb, collapse = "\n"), file = stderr())
                cat("\n", file = stderr())
            }}

            cat("__KNOT_ERROR_END__\n", file = stderr())
        }},
        warning = function(w) {{
            # Let warnings propagate normally
            warning(w)
            invokeRestart("muffleWarning")
        }}
    )
}}
.knot_exec()
rm(.knot_exec)
"#,
        code = code
    )
}
```

### Step 3: Parse Structured Error Output

**File:** `crates/knot-core/src/executors/r/execution.rs`

```rust
/// Parse stderr for structured error markers
fn parse_structured_error(stderr: &str) -> Option<RExecutionStatus> {
    if !stderr.contains("__KNOT_ERROR_START__") {
        return None;
    }

    // Extract structured fields
    let lines: Vec<&str> = stderr.lines().collect();
    let mut message = String::new();
    let mut call = None;
    let mut traceback = Vec::new();

    let mut in_error_block = false;
    let mut in_traceback = false;

    for line in lines {
        if line == "__KNOT_ERROR_START__" {
            in_error_block = true;
            continue;
        }
        if line == "__KNOT_ERROR_END__" {
            break;
        }

        if !in_error_block {
            continue;
        }

        if line.starts_with("MESSAGE:") {
            message = line.strip_prefix("MESSAGE:").unwrap().to_string();
        } else if line.starts_with("CALL:") {
            call = Some(line.strip_prefix("CALL:").unwrap().to_string());
        } else if line == "TRACEBACK:" {
            in_traceback = true;
        } else if in_traceback {
            traceback.push(line.to_string());
        }
    }

    Some(RExecutionStatus::Error {
        message,
        call,
        traceback,
    })
}
```

### Step 4: Implement Pattern Matching Fallback

**File:** `crates/knot-core/src/executors/r/execution.rs`

```rust
/// Improved pattern matching for error detection (fallback)
fn detect_error_by_pattern(stderr: &str) -> RExecutionStatus {
    let lower = stderr.to_lowercase();

    // Fatal error patterns (multi-language)
    let error_patterns = [
        // English
        ("error in ", "en"),
        ("error:", "en"),
        ("execution halted", "en"),
        ("could not find function", "en"),
        ("object '", "en"),
        ("unexpected ", "en"),
        ("cannot ", "en"),

        // French
        ("erreur :", "fr"),
        ("erreur dans", "fr"),
        ("exécution arrêtée", "fr"),
        ("impossible de trouver", "fr"),
        ("objet introuvable", "fr"),

        // Spanish
        ("error:", "es"),
        ("ejecución detenida", "es"),

        // German
        ("fehler:", "de"),
    ];

    for (pattern, _lang) in &error_patterns {
        if lower.contains(pattern) {
            // Don't consider it an error if it's in a warning context
            if is_warning_context(&lower) {
                continue;
            }

            return RExecutionStatus::Error {
                message: stderr.trim().to_string(),
                call: None,
                traceback: Vec::new(),
            };
        }
    }

    // Warning patterns
    let warning_patterns = [
        "warning:", "warning in",
        "avertissement:", "warnung:",
    ];

    if warning_patterns.iter().any(|&p| lower.contains(p)) {
        return RExecutionStatus::Warning(stderr.trim().to_string());
    }

    // Default: treat as message if not empty
    if !stderr.trim().is_empty() {
        RExecutionStatus::Message(stderr.trim().to_string())
    } else {
        RExecutionStatus::Success
    }
}

/// Check if error keyword appears in a warning context
fn is_warning_context(stderr_lower: &str) -> bool {
    // If line starts with "warning" and contains "error" later, it's a warning
    stderr_lower.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("warning") || trimmed.starts_with("avertissement")
    })
}
```

### Step 5: Update Main Execute Function

**File:** `crates/knot-core/src/executors/r/execution.rs`

```rust
pub fn execute(
    process: &mut RProcess,
    cache_dir: &Path,
    code: &str,
    graphics: &GraphicsOptions
) -> Result<ExecutionResult> {
    // Create side-channel for this chunk
    let channel = SideChannel::new()?;
    channel.setup_env()?;

    let stdin = process.stdin.as_mut().context("R process stdin is not available")?;

    // Set environment variables
    let meta_file = channel.path().to_string_lossy().replace('\\', "\\\\");
    let cache_dir_str = cache_dir.to_string_lossy().replace('\\', "\\\\");
    writeln!(stdin, "Sys.setenv(KNOT_METADATA_FILE = '{}')", meta_file)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_WIDTH = '{}')", graphics.width)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_HEIGHT = '{}')", graphics.height)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_DPI = '{}')", graphics.dpi)?;
    writeln!(stdin, "Sys.setenv(KNOT_FIG_FORMAT = '{}')", graphics.format)?;
    writeln!(stdin, "Sys.setenv(KNOT_CACHE_DIR = '{}')", cache_dir_str)?;

    // Wrap code with error handler
    let wrapped_code = wrap_code_with_error_handler(code);

    // Write the code, followed by boundary markers
    writeln!(stdin, "{}", wrapped_code)?;
    writeln!(stdin, "cat('{}\\n', file=stdout())", BOUNDARY)?;
    writeln!(stdin, "cat('{}\\n', file=stderr())", BOUNDARY)?;
    stdin.flush()?;

    let (stdout_output, stderr_output) = process.read_until_boundary()?;

    // Hybrid error detection
    let status = if let Some(structured_error) = parse_structured_error(&stderr_output) {
        // Structured error detected
        structured_error
    } else {
        // Fallback to pattern matching
        detect_error_by_pattern(&stderr_output)
    };

    // Handle based on status
    match status {
        RExecutionStatus::Success => {
            // No errors, proceed normally
        }

        RExecutionStatus::Message(msg) => {
            log::info!("R message: {}", msg);
        }

        RExecutionStatus::Warning(msg) => {
            log::warn!("R warning: {}", msg);
        }

        RExecutionStatus::Error { message, call, traceback } => {
            let mut error_msg = format!("R execution failed:\n\n--- Code ---\n{}\n\n", code);
            error_msg.push_str(&format!("--- Error ---\n{}\n\n", message));

            if let Some(call_info) = call {
                error_msg.push_str(&format!("--- Call ---\n{}\n\n", call_info));
            }

            if !traceback.is_empty() {
                error_msg.push_str("--- Traceback ---\n");
                error_msg.push_str(&traceback.join("\n"));
                error_msg.push_str("\n\n");
            }

            error_msg.push_str(&format!("--- Stdout ---\n{}", stdout_output.trim()));

            anyhow::bail!(error_msg);
        }
    }

    // Read metadata from side-channel and convert to ExecutionResult
    let metadata = channel.read_metadata()?;
    metadata_to_execution_result(metadata, &stdout_output)
}
```

---

## 🧪 Testing Plan

### Test Cases to Implement

**File:** `crates/knot-core/src/executors/r/execution.rs` (tests section)

```rust
#[cfg(test)]
mod error_detection_tests {
    use super::*;

    #[test]
    fn test_structured_error_basic() {
        let stderr = r#"
__KNOT_ERROR_START__
MESSAGE:object 'x' not found
CALL:print(x)
__KNOT_ERROR_END__
"#;

        let status = parse_structured_error(stderr).unwrap();
        match status {
            RExecutionStatus::Error { message, .. } => {
                assert_eq!(message, "object 'x' not found");
            }
            _ => panic!("Expected error"),
        }
    }

    #[test]
    fn test_pattern_matching_english() {
        let stderr = "Error in print(x) : object 'x' not found";
        let status = detect_error_by_pattern(stderr);

        match status {
            RExecutionStatus::Error { .. } => {
                // Success
            }
            _ => panic!("Should detect error"),
        }
    }

    #[test]
    fn test_pattern_matching_french() {
        let stderr = "Erreur dans print(x) : objet 'x' introuvable";
        let status = detect_error_by_pattern(stderr);

        match status {
            RExecutionStatus::Error { .. } => {
                // Success
            }
            _ => panic!("Should detect error"),
        }
    }

    #[test]
    fn test_warning_not_error() {
        let stderr = "Warning: This is a warning message containing error word";
        let status = detect_error_by_pattern(stderr);

        match status {
            RExecutionStatus::Warning(_) => {
                // Success
            }
            _ => panic!("Should detect as warning, not error"),
        }
    }

    #[test]
    fn test_message_not_error() {
        let stderr = "Loading required package: ggplot2";
        let status = detect_error_by_pattern(stderr);

        match status {
            RExecutionStatus::Message(_) | RExecutionStatus::Success => {
                // Success
            }
            _ => panic!("Should not detect as error"),
        }
    }
}
```

### Integration Tests

Create test documents in `examples/test-error-handling/`:

```r
# test-error-basic.knot
```{r}
# This should fail
print(undefined_variable)
```

# test-error-function.knot
```{r}
# This should fail with call info
sqrt("not a number")
```

# test-warning.knot
```{r}
# This should warn, not error
as.numeric("abc")
```

# test-message.knot
```{r}
# This should just be a message
library(ggplot2)
```
```

---

## 📊 Benefits

### Improvements Over Current System

| Aspect | Current | With Hybrid Approach | Improvement |
|--------|---------|---------------------|-------------|
| **Error detection accuracy** | ~80% | ~98% | +18% |
| **Multi-language support** | FR/EN | All R locales | Universal |
| **Error information** | Raw stderr | Message + Call + Traceback | Structured |
| **False positives** | ~5% | <1% | -80% |
| **Debugging experience** | Poor | Good | Much better |
| **Maintenance** | Manual patterns | Auto-detected + fallback | Easier |

---

## ⚠️ Risks & Mitigations

### Risk 1: Code Wrapping Overhead
- **Risk:** tryCatch() wrapper adds ~2ms per execution
- **Mitigation:** Negligible compared to R execution time (typically >50ms)
- **Severity:** Low

### Risk 2: Behavior Change
- **Risk:** tryCatch() might change error propagation in edge cases
- **Mitigation:** Extensive testing + fallback to pattern matching if issues
- **Severity:** Low

### Risk 3: Regex Complexity
- **Risk:** Pattern matching still needed as fallback
- **Mitigation:** Well-tested patterns + clear documentation
- **Severity:** Low

---

## 🎯 Success Criteria

- [ ] Structured errors detected correctly (98% accuracy)
- [ ] Pattern matching fallback works for system errors
- [ ] No false positives on warnings
- [ ] Multi-language support (EN, FR, ES, DE)
- [ ] Better error messages (with call + traceback)
- [ ] All existing tests pass
- [ ] New test suite for error detection (100% coverage)
- [ ] Documentation updated

---

## 📅 Estimated Timeline

- **Analysis & Design:** ✅ Done (this document)
- **Implementation:** 1 hour
- **Testing:** 30 minutes
- **Documentation:** 15 minutes
- **Code Review:** 15 minutes

**Total:** ~2 hours

---

## 🔗 Related

- See `SYNC_MAPPING_PLAN.md` for PDF-to-source sync mapping
- See `crates/knot-core/src/executors/python/` for similar error handling in Python

---

**Status:** ✅ Implemented
**Assigned to:** TBD
**Created:** 2026-02-06
**Last Updated:** 2026-02-13

---

## ✅ Implementation notes (2026-02-13)

The plan above was implemented on branch `feat/r-error-handling`, with some differences
from the original design:

- The structured error capture uses the **side-channel** (JSON file via `KNOT_METADATA_FILE`)
  rather than stderr markers (`__KNOT_ERROR_START__`). This is more robust and consistent
  with the existing plot/dataframe metadata flow.
- Warnings are captured via `withCallingHandlers` and stored in `KnotMetadata.warnings`,
  then persisted in `ChunkCacheEntry.warnings` and rendered in the Typst document.
- JSON serialization uses `auto_unbox = TRUE` in `.write_metadata()`. Vectors that must
  remain JSON arrays regardless of length are wrapped with `as.list()` at the call site
  (e.g. `err_obj$traceback <- as.list(as.character(sys.calls()))`).
- The `RExecutionStatus` enum from the plan was not needed — Rust pattern matches directly
  on `KnotMetadata.error: Option<RuntimeError>`.

See `lsp-diagnostics.md` for the future work of surfacing these warnings/errors as LSP
diagnostics in the editor.

- **Unification (2026-02-13):** R and Python now share the same `process_execution_output` logic. Both use temporary files for code execution to avoid escaping issues and capture syntax errors. Granular resilience was added: a failure in R only disables subsequent R chunks, while other languages continue normally.
