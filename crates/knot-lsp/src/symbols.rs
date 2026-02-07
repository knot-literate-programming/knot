// Document Symbols for Knot documents
//
// Provides hierarchical structure information:
// - List of code chunks with names and positions
// - Chunk metadata (language, options)

use knot_core::parser::parse_document;
use tower_lsp::lsp_types::*;

/// Generate document symbols (chunk list) for a document
pub fn get_document_symbols(text: &str) -> Option<Vec<DocumentSymbol>> {
    // Use the unified parser
    let doc = parse_document(text);

    let mut symbols = Vec::new();

    for chunk in &doc.chunks {
        // Create a symbol for each chunk
        let name = chunk
            .name
            .clone()
            .unwrap_or_else(|| format!("Unnamed {} chunk", chunk.language));

        // Use the chunk's range
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
        if matches!(chunk.options.eval, Some(false)) {
            details.push("eval: false".to_string());
        }
        if matches!(chunk.options.echo, Some(false)) {
            details.push("echo: false".to_string());
        }
        if matches!(chunk.options.output, Some(false)) {
            details.push("output: false".to_string());
        }
        if matches!(chunk.options.cache, Some(false)) {
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
            kind: SymbolKind::FUNCTION,
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
