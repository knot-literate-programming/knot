// Inline Expression Output Formatting
//
// Formats R output for inline display:
// - Scalars: Extract value without [1] prefix (e.g., "150", "TRUE", "Alice")
// - Short vectors: Wrap in backticks (e.g., "`[1] 1 2 3 4 5`")
// - Complex output: Reject with descriptive error

use anyhow::Result;

/// Format R output for inline display
pub fn format_inline_output(output: &str) -> Result<String> {
    let trimmed = output.trim();

    // Allow empty results (e.g. from assignments like x <- 1)
    if trimmed.is_empty() {
        return Ok(String::new());
    }

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
            if trimmed.len() > 100 {
                &trimmed[..100]
            } else {
                trimmed
            }
        )
    }
}

/// Extract scalar value from R output
///
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

/// Check if R output is a short vector
///
/// Characteristics:
/// - Single line
/// - Starts with [1] (R vector notation)
/// - More than one value after [1]
/// - Reasonable length (< 80 chars for inline display)
fn is_short_vector_output(s: &str) -> bool {
    if s.contains('\n') || !s.starts_with("[1]") || s.len() >= 80 {
        return false;
    }

    // Check if there are multiple values (not handled by extract_scalar_value)
    let after_prefix = s[3..].trim();
    after_prefix.split_whitespace().nth(1).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_scalar_value() {
        assert_eq!(extract_scalar_value("[1] 150"), Some("150".to_string()));
        assert_eq!(extract_scalar_value("[1] TRUE"), Some("TRUE".to_string()));
        assert_eq!(
            extract_scalar_value("[1] \"Alice\""),
            Some("Alice".to_string())
        );
    }

    #[test]
    fn test_is_short_vector() {
        assert!(is_short_vector_output("[1] 1 2 3 4 5"));
        assert!(is_short_vector_output("[1] \"a\" \"b\" \"c\""));
        assert!(!is_short_vector_output("[1] 150")); // Single value
        assert!(!is_short_vector_output(&"[1] ".repeat(100))); // Too long
    }
}
