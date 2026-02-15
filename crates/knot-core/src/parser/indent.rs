//! Indentation Utilities
//!
//! Provides functions to normalize and restore indentation for code chunks.

/// Removes the common leading whitespace from each line of the text.
/// Returns the cleaned text and the string that was removed.
pub fn dedent(text: &str) -> (String, String) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (String::new(), String::new());
    }

    // 1. Find the minimum common indentation
    let mut min_indent: Option<&str> = None;

    for line in &lines {
        if line.trim().is_empty() {
            continue; // Skip empty or whitespace-only lines
        }

        let current_indent = line
            .find(|c: char| !c.is_whitespace())
            .map(|idx| &line[..idx])
            .unwrap_or("");

        match min_indent {
            None => min_indent = Some(current_indent),
            Some(indent) => {
                // Find the common prefix between current_indent and existing min_indent
                let mut common_len = 0;
                for (c1, c2) in indent.chars().zip(current_indent.chars()) {
                    if c1 == c2 {
                        common_len += c1.len_utf8();
                    } else {
                        break;
                    }
                }
                min_indent = Some(&indent[..common_len]);
            }
        }
    }

    let indent_str = min_indent.unwrap_or("").to_string();

    // 2. Strip the indentation from each line
    if indent_str.is_empty() {
        return (text.to_string(), String::new());
    }

    let mut result = Vec::with_capacity(lines.len());
    for line in lines {
        if line.trim().is_empty() {
            result.push(""); // Keep empty lines empty
        } else {
            result.push(line.strip_prefix(&indent_str).unwrap_or(line));
        }
    }

    (result.join("
"), indent_str)
}

/// Adds the given indentation prefix to each line of the text.
pub fn indent(text: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len() + prefix.len() * 10);
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        if !line.trim().is_empty() {
            result.push_str(prefix);
        }
        result.push_str(line);
    }
    
    // Preserve trailing newline if present
    if text.ends_with('\n') {
        result.push('\n');
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedent_basic() {
        let input = "  line 1
    line 2
  line 3";
        let (output, indent) = dedent(input);
        assert_eq!(indent, "  ");
        assert_eq!(output, "line 1
  line 2
line 3");
    }

    #[test]
    fn test_dedent_no_indent() {
        let input = "line 1
  line 2";
        let (output, indent) = dedent(input);
        assert_eq!(indent, "");
        assert_eq!(output, input);
    }

    #[test]
    fn test_indent_basic() {
        let input = "line 1
  line 2";
        let output = indent(input, "  ");
        assert_eq!(output, "  line 1
    line 2");
    }
}
