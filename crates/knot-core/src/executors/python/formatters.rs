// Python Output Formatters
//
// Formats Python objects into Typst-compatible strings.

/// Formats a string result for inline display
pub fn format_inline_output(output: &str) -> String {
    // For now, simple trim.
    // Future: handle list/dict conversion to Typst syntax.
    output.trim().to_string()
}
