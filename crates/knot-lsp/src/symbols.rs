// Document Symbols for Knot documents
//
// Provides hierarchical structure information:
// - List of code chunks with names and positions
// - Chunk metadata (language, options)

use knot_core::Document;
use tower_lsp::lsp_types::*;

/// Generate document symbols (chunk list) for a document
pub fn get_document_symbols(text: &str) -> Option<Vec<DocumentSymbol>> {
    // Try to parse the document
    let doc = Document::parse(text.to_string()).ok()?;

    let mut symbols = Vec::new();

    for chunk in &doc.chunks {
        // Create a symbol for each chunk
        let name = chunk
            .name
            .clone()
            .unwrap_or_else(|| format!("Unnamed {} chunk", chunk.language));

        // Use the chunk's range (from parser.rs)
        let range = Range {
            start: Position {
                line: chunk.range.start.line as u32,
                character: chunk.range.start.column as u32,
            },
            end: Position {
                line: chunk.range.end.line as u32,
                character: chunk.range.end.column as u32,
            },
        };

        // Use code range for selection
        let selection_range = Range {
            start: Position {
                line: chunk.code_range.start.line as u32,
                character: chunk.code_range.start.column as u32,
            },
            end: Position {
                line: chunk.code_range.end.line as u32,
                character: chunk.code_range.end.column as u32,
            },
        };

        // Create detail string with chunk options
        let mut details = vec![format!("Language: {}", chunk.language)];
        if !chunk.options.eval {
            details.push("eval: false".to_string());
        }
        if !chunk.options.echo {
            details.push("echo: false".to_string());
        }
        if !chunk.options.output {
            details.push("output: false".to_string());
        }
        if !chunk.options.cache {
            details.push("cache: false".to_string());
        }
        let detail = if details.len() > 1 {
            Some(details.join(", "))
        } else {
            Some(details[0].clone())
        };

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name,
            detail,
            kind: SymbolKind::FUNCTION, // Use FUNCTION kind for code chunks
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: None,
        });
    }

    if symbols.is_empty() {
        None
    } else {
        Some(symbols)
    }
}
