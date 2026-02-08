// Python Output Formatters
//
// Formats Python objects into Typst-compatible strings.

use anyhow::Result;

/// Formats a string result for inline display
pub fn format_inline_output(output: &str) -> Result<String> {
    let trimmed = output.trim();

    // 1. Safety limit: reject very long outputs in inline expressions
    // (Parity with R implementation)
    if trimmed.len() > 100 {
        anyhow::bail!(
            "Inline expression result is too complex or long.\n\
             Result: {}\n\
             Inline expressions should return simple scalar values or short collections.",
            if trimmed.len() > 100 {
                &trimmed[..100]
            } else {
                trimmed
            }
        );
    }

    // 2. Check if it's a collection (List, Tuple, Set, Dict)
    // Python's print() uses [], (), {} for these.
    // We wrap them in backticks to show them as "code" in the document.
    if is_python_collection(trimmed) {
        return Ok(format!("`{}`", trimmed));
    }

    // 3. Default: return as plain text (perfect for numbers and simple strings)
    Ok(trimmed.to_string())
}

/// Simple detection of Python collection strings
fn is_python_collection(s: &str) -> bool {
    (s.starts_with('[') && s.ends_with(']'))
        || (s.starts_with('(') && s.ends_with(')'))
        || (s.starts_with('{') && s.ends_with('}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_inline_scalar() {
        assert_eq!(format_inline_output("25").unwrap(), "25");
        assert_eq!(format_inline_output("Alice").unwrap(), "Alice");
        assert_eq!(format_inline_output("True").unwrap(), "True");
    }

    #[test]
    fn test_format_inline_collection() {
        assert_eq!(format_inline_output("[1, 2, 3]").unwrap(), "`[1, 2, 3]`");
        assert_eq!(format_inline_output("(1, 2)").unwrap(), "`(1, 2)`");
        assert_eq!(format_inline_output("{'a': 1}").unwrap(), "`{'a': 1}`");
    }

    #[test]
    fn test_format_inline_too_long() {
        let long_str = "a".repeat(101);
        assert!(format_inline_output(&long_str).is_err());
    }
}
