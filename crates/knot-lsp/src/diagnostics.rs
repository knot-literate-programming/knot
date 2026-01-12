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
