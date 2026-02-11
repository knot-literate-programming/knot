/// Error formatting utilities for intelligent code context display
use regex::Regex;

/// Extract line number from error messages
///
/// Supports:
/// - Python: "line 15, in <module>" (looks for user code, not wrapper)
/// - R: various error formats (though R is less consistent)
fn extract_error_line(error_msg: &str) -> Option<usize> {
    // Python traceback format: look for "line X, in <module>" which is user code
    // (not "line X, in main" which is the wrapper)
    let python_module_re = Regex::new(r#"line (\d+), in <module>"#).ok()?;
    if let Some(cap) = python_module_re.captures(error_msg) {
        return cap.get(1)?.as_str().parse().ok();
    }

    // Fallback: any "line X," but prefer later occurrences
    let python_re = Regex::new(r"line (\d+),").ok()?;
    let all_matches: Vec<_> = python_re.captures_iter(error_msg).collect();
    if let Some(last_match) = all_matches.last() {
        return last_match.get(1)?.as_str().parse().ok();
    }

    // R format: "at line 15"
    let r_re = Regex::new(r"at line (\d+)").ok()?;
    if let Some(cap) = r_re.captures(error_msg) {
        return cap.get(1)?.as_str().parse().ok();
    }

    None
}

/// Format code with intelligent context around the error
///
/// If an error line is found in the error message, shows context around that line.
/// Otherwise, shows the beginning of the code with a reasonable limit.
pub fn format_code_with_context(code: &str, error_msg: &str, context_lines: usize) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let total_lines = lines.len();

    // If code is short enough, just return it all
    if total_lines <= 10 {
        return code.to_string();
    }

    // Try to find the error line
    if let Some(error_line) = extract_error_line(error_msg) {
        // error_line is 1-indexed, convert to 0-indexed
        let error_idx = error_line.saturating_sub(1);

        if error_idx < total_lines {
            let start = error_idx.saturating_sub(context_lines);
            let end = (error_idx + context_lines + 1).min(total_lines);

            let mut result = String::new();

            // Show indication if we're skipping the beginning
            if start > 0 {
                result.push_str(&format!("... ({} lines above)\n", start));
            }

            // Show the context lines with line numbers
            for (i, line) in lines[start..end].iter().enumerate() {
                let line_num = start + i + 1; // 1-indexed for display
                if line_num == error_line {
                    result.push_str(&format!(">>> {}: {}\n", line_num, line));
                } else {
                    result.push_str(&format!("    {}: {}\n", line_num, line));
                }
            }

            // Show indication if we're skipping the end
            if end < total_lines {
                result.push_str(&format!("... ({} lines below)", total_lines - end));
            }

            return result.trim_end().to_string();
        }
    }

    // Fallback: show first N lines with better limit
    let show_lines = 15.min(total_lines);
    let mut result = lines[..show_lines].join("\n");

    if total_lines > show_lines {
        result.push_str(&format!(
            "\n... ({} lines not shown)",
            total_lines - show_lines
        ));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_python_line() {
        let error = r#"Traceback (most recent call last):
  File "<string>", line 15, in <module>
NameError: name 'x' is not defined"#;
        assert_eq!(extract_error_line(error), Some(15));
    }

    #[test]
    fn test_extract_r_line() {
        let error = "Error at line 10: object 'x' not found";
        assert_eq!(extract_error_line(error), Some(10));
    }

    #[test]
    fn test_format_short_code() {
        let code = "line1\nline2\nline3";
        let error = "some error";
        let result = format_code_with_context(code, error, 2);
        assert_eq!(result, code);
    }

    #[test]
    fn test_format_with_error_line() {
        let code = (1..=20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let error = "Traceback: line 10, in <module>";
        let result = format_code_with_context(&code, error, 2);

        assert!(result.contains(">>> 10:"));
        assert!(result.contains("line 8"));
        assert!(result.contains("line 12"));
    }
}
