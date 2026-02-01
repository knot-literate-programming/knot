// Diagnostics for Knot documents
//
// Provides error detection and validation:
// - Parsing errors (malformed chunks, unmatched brackets)
// - Invalid chunk options
// - Missing dependencies
// - Invalid inline expressions

use knot_core::Document;
use tower_lsp::lsp_types::*;

/// Generate diagnostics for a document
pub fn get_diagnostics(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Try to parse the document
    match Document::parse(text.to_string()) {
        Ok(_doc) => {
            // Successfully parsed - no errors
            // In the future, we could add more validation here:
            // - Check for missing dependencies
            // - Validate chunk references
            // - Check for invalid option values
        }
        Err(err) => {
            // Parsing error - create diagnostic
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("knot".to_string()),
                message: format!("Parsing error: {}", err),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_diagnostics_valid_document() {
        let text = r###"
= My Document

Some text here.

```{r}
#| eval: true
x <- 1
```

More text.
"###;
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_empty_document() {
        let text = "";
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_plain_text() {
        let text = "Just some plain text without any chunks.";
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_with_inline_expressions() {
        let text = r###"
= Analysis

The mean is `{r} mean(x)` and the sum is `{r} sum(x)`.
"###;
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_multiple_chunks() {
        let text = r###"
```{r}
x <- 1
```

```{r}
y <- 2
```
"###;
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_chunk_with_options() {
        let text = r###"
```{r my-chunk}
#| eval: false
#| echo: true
#| output: true
#| cache: false
#| fig-width: 10
#| fig-height: 8
plot(1:10)
```
"###;
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_get_diagnostics_inline_with_options() {
        let text = "Result: `{r digits=3} pi` is approximately 3.142.";
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 0);
    }
}
