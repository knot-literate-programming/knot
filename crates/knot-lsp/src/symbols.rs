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

        // Use the chunk's range for both full and selection ranges
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

        let selection_range = range;

        // Create detail string with chunk options
        let mut details = vec![format!("Language: {}", chunk.language)];
        if matches!(chunk.options.eval, Some(false)) {
            details.push("eval: false".to_string());
        }
        if let Some(show) = &chunk.options.show {
            use knot_core::parser::Show;
            match show {
                Show::Code => details.push("show: code".to_string()),
                Show::Output => details.push("show: output".to_string()),
                Show::None => details.push("show: none".to_string()),
                Show::Both => {} // default, don't show
            }
        }
        if matches!(chunk.options.cache, Some(false)) {
            details.push("cache: false".to_string());
        }
        let detail = if details.len() > 1 {
            Some(details.join(", "))
        } else {
            Some(details[0].clone())
        };

        // The `deprecated` field on `DocumentSymbol` is itself deprecated in the LSP spec,
        // but the lsp-types crate still requires it to construct the struct.
        #[expect(deprecated)]
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
