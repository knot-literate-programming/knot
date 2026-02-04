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
        Ok(doc) => {
            // Check for errors in chunks
            for chunk in doc.chunks {
                for error in chunk.errors {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: chunk.range.start.line as u32,
                                character: chunk.range.start.column as u32,
                            },
                            end: Position {
                                line: chunk.range.start.line as u32,
                                character: (chunk.range.start.column + 3) as u32, // Highlight ```
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: None,
                        source: Some("knot".to_string()),
                        message: error,
                        ..Diagnostic::default()
                    });
                }
            }

            // Check for errors in inline expressions
            for inline in doc.inline_exprs {
                for error in inline.errors {
                    // We need to convert byte offsets to line/col for the diagnostic
                    let (line, col) = byte_offset_to_line_col(text, inline.start);
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: line as u32,
                                character: col as u32,
                            },
                            end: Position {
                                line: line as u32,
                                character: (col + 1) as u32, // Highlight `
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: None,
                        source: Some("knot".to_string()),
                        message: error,
                        ..Diagnostic::default()
                    });
                }
            }
        }
        Err(err) => {
            // Parsing error - create diagnostic at the beginning
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("knot".to_string()),
                message: format!("Parsing error: {}", err),
                ..Diagnostic::default()
            });
        }
    }

    diagnostics
}

/// Helper to convert byte offset to line/col (UTF-16 aware for LSP)
fn byte_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    
    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16();
        }
    }
    (line, col)
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
    fn test_get_diagnostics_invalid_options() {
        let text = r###"```{r}
#| eval: maybe
#| unknown: true
1 + 1
```"###;
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("Option 'eval'"));
        assert!(diagnostics[1].message.contains("Unknown option"));
    }

    #[test]
    fn test_get_diagnostics_invalid_inline_options() {
        let text = "Result: `{r digits=abc unknown=1} pi`";
        let diagnostics = get_diagnostics(text);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("Option 'digits'"));
        assert!(diagnostics[1].message.contains("Unknown option"));
    }
}
