use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::state::ServerState;
use knot_core::Document;

pub async fn handle_formatting(state: &ServerState, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
    // Check if formatter is available
    let formatter = match &state.formatter {
        Some(f) => f,
        None => return Ok(None),
    };

    // Get document text
    let documents = state.documents.read().await;
    let text = match documents.get(&params.text_document.uri) {
        Some(text) => text.clone(),
        None => return Ok(None),
    };
    drop(documents);

    // Parse document
    let doc = match Document::parse(text) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    // Format each R chunk
    let mut edits = Vec::new();
    for chunk in &doc.chunks {
        if chunk.language == "r" {
            match formatter.format_r_code(&chunk.code).await {
                Ok(formatted) => {
                    // Only create edit if code changed
                    if formatted.trim() != chunk.code.trim() {
                        edits.push(TextEdit {
                            range: Range {
                                start: Position {
                                    line: chunk.code_range.start.line as u32,
                                    character: chunk.code_range.start.column as u32,
                                },
                                end: Position {
                                    line: chunk.code_range.end.line as u32,
                                    character: chunk.code_range.end.column as u32,
                                },
                            },
                            new_text: formatted,
                        });
                    }
                }
                Err(_) => {} // Ignore formatting errors for now
            }
        }
    }

    Ok(if edits.is_empty() { None } else { Some(edits) })
}

pub async fn handle_on_type_formatting(state: &ServerState, params: DocumentOnTypeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
    // Only format on newline
    if params.ch != "\n" {
        return Ok(None);
    }

    // Check if formatter is available
    let formatter = match &state.formatter {
        Some(f) => f,
        None => return Ok(None),
    };

    // Get document text
    let documents = state.documents.read().await;
    let text = match documents.get(&params.text_document_position.text_document.uri) {
        Some(text) => text.clone(),
        None => return Ok(None),
    };
    drop(documents);

    // Parse document
    let doc = match Document::parse(text) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    // Find the chunk containing the cursor position
    let cursor_line = params.text_document_position.position.line as usize;
    let current_chunk = doc.chunks.iter().find(|chunk| {
        chunk.language == "r" 
            && chunk.range.start.line <= cursor_line 
            && chunk.range.end.line >= cursor_line
    });

    // Format only the current chunk
    if let Some(chunk) = current_chunk {
        match formatter.format_r_code(&chunk.code).await {
            Ok(formatted) => {
                if formatted.trim() != chunk.code.trim() {
                    return Ok(Some(vec![TextEdit {
                        range: Range {
                            start: Position {
                                line: chunk.code_range.start.line as u32,
                                character: chunk.code_range.start.column as u32,
                            },
                            end: Position {
                                line: chunk.code_range.end.line as u32,
                                character: chunk.code_range.end.column as u32,
                            },
                        },
                        new_text: formatted,
                    }]));
                }
            }
            Err(_) => {} // Ignore formatting errors for now
        }
    }

    Ok(None)
}
