/// Error formatting utilities for intelligent code context display
use regex::Regex;

/// Extract line number from error messages or traceback lines
///
/// Supports:
/// - Python: "line 15, in <module>" (looks for user code, not wrapper)
/// - R: various error formats (though R is less consistent)
pub fn extract_error_line(error_msg: &str) -> Option<usize> {
    // Python traceback format: look for "line X, in <module>" which is user code
    let python_module_re = Regex::new(r#"line (\d+), in <module>"#).ok()?;
    if let Some(cap) = python_module_re.captures(error_msg) {
        return cap.get(1)?.as_str().parse().ok();
    }

    // Python fallback: "line X," but prefer later occurrences (inner frames)
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

/// Extract the error line number from a structured traceback
pub fn extract_line_from_traceback(traceback: &[String]) -> Option<usize> {
    // Search from the end (most specific frame) to the beginning
    for line in traceback.iter().rev() {
        if let Some(num) = extract_error_line(line) {
            return Some(num);
        }
    }
    None
}

/// Format code with intelligent context around the error
pub fn format_code_with_context(code: &str, error_msg: &str, context_lines: usize) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let total_lines = lines.len();

    if total_lines <= 10 {
        return code.to_string();
    }

    if let Some(error_line) = extract_error_line(error_msg) {
        let error_idx = error_line.saturating_sub(1);

        if error_idx < total_lines {
            let start = error_idx.saturating_sub(context_lines);
            let end = (error_idx + context_lines + 1).min(total_lines);

            let mut result = String::new();
            if start > 0 {
                result.push_str(&format!("... ({} lines above)\n", start));
            }

            for (i, line) in lines[start..end].iter().enumerate() {
                let line_num = start + i + 1;
                if line_num == error_line {
                    result.push_str(&format!(">>> {}: {}\n", line_num, line));
                } else {
                    result.push_str(&format!("    {}: {}\n", line_num, line));
                }
            }

            if end < total_lines {
                result.push_str(&format!("... ({} lines below)", total_lines - end));
            }

            return result.trim_end().to_string();
        }
    }

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
    fn test_extract_from_traceback() {
        let tb = vec![
            "line 1, in <module>".to_string(),
            "line 5, in nested_func".to_string(),
        ];
        assert_eq!(extract_line_from_traceback(&tb), Some(5));
    }
}
