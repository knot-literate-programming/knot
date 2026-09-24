//! Helpers shared by the R and Python inline formatters.

/// Maximum number of characters accepted in an inline result, and shown in
/// the error preview when a result is rejected.
pub(crate) const INLINE_MAX_CHARS: usize = 100;

/// Error returned when an inline result is too long or complex to display.
///
/// The preview is truncated on a character boundary, so multi-byte results
/// never cause a panic. `expected` completes "should return simple scalar
/// values or …" with the language's collection vocabulary.
pub(crate) fn inline_result_too_complex(result: &str, expected: &str) -> anyhow::Error {
    let preview = match result.char_indices().nth(INLINE_MAX_CHARS) {
        Some((end, _)) => &result[..end],
        None => result,
    };
    anyhow::anyhow!(
        "Inline expression result is too complex or long.\n\
         Result: {preview}\n\
         Inline expressions should return simple scalar values or {expected}."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(error: &anyhow::Error) -> String {
        let message = error.to_string();
        let line = message.lines().nth(1).unwrap();
        line.strip_prefix("Result: ").unwrap().to_string()
    }

    #[test]
    fn preview_keeps_short_results_whole() {
        let error = inline_result_too_complex("a\nb", "short vectors");
        assert!(error.to_string().contains("Result: a\nb"));
    }

    #[test]
    fn preview_truncates_on_character_boundary() {
        // Byte 100 falls inside a two-byte character.
        let result = format!("a{}", "é".repeat(150));
        let error = inline_result_too_complex(&result, "short vectors");
        let preview = preview(&error);
        assert_eq!(preview.chars().count(), INLINE_MAX_CHARS);
        assert!(result.starts_with(&preview));
    }
}
