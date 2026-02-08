// Diagnostics for Knot documents
//
// Provides error detection and validation:
// - Parsing errors (malformed chunks, unmatched brackets)
// - Invalid chunk options
// - Missing dependencies
// - Invalid inline expressions

use knot_core::parser::parse_document;
use tower_lsp::lsp_types::*;

/// Generate diagnostics for a document
pub fn get_diagnostics(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Use the unified parser
    let doc = parse_document(text);

    // Check for global document errors
    for error in doc.errors {
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
            source: Some("knot".to_string()),
            message: error,
            ..Diagnostic::default()
        });
    }

    // Check for errors in chunks
    for chunk in doc.chunks {
        for error in chunk.errors {
            // Use precise line offset if available, otherwise highlight the header
            let target_line = if let Some(offset) = error.line_offset {
                chunk.range.start.line + offset
            } else {
                chunk.range.start.line
            };

            let line_text = text.lines().nth(target_line).unwrap_or("");
            let line_len = line_text.len() as u32;

            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: target_line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: target_line as u32,
                        character: line_len,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                source: Some("knot".to_string()),
                message: error.message,
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
                message: error.message,
                ..Diagnostic::default()
            });
        }
    }

    diagnostics
}

/// Helper to convert byte offset to line/col (UTF-16 aware for LSP)
pub fn byte_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
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
