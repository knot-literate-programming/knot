use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_hover(state: &ServerState, params: HoverParams) -> Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;

    // 1. Get document text and mapper
    let (text, mapper) = {
        let docs = state.documents.read().await;
        let mappers = state.mappers.read().await;
        match (docs.get(uri), mappers.get(uri)) {
            (Some(t), Some(m)) => (t.clone(), m.clone()),
            _ => return Ok(None),
        }
    };

    // 2. Determine if we are in a chunk
    if mapper.is_position_in_chunk(pos) {
        // Knot (R chunk) Hover
        let doc = match Document::parse(text) {
            Ok(doc) => doc,
            Err(_) => return Ok(None),
        };

        let line = pos.line as usize;
        let current_chunk = doc
            .chunks
            .iter()
            .find(|c| c.range.start.line <= line && c.range.end.line >= line);

        if let Some(chunk) = current_chunk {
            let name = chunk.name.as_deref().unwrap_or("unnamed");
            let mut content = format!("### Knot Chunk: `{}`\n\n", name);
            content.push_str(&format!("- **Language**: `{}`\n", chunk.language));
            content.push_str(&format!("- **Eval**: `{}`\n", chunk.options.eval));
            content.push_str(&format!("- **Echo**: `{}`\n", chunk.options.echo));
            content.push_str(&format!("- **Cache**: `{}`\n", chunk.options.cache));

            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(Range {
                    start: Position {
                        line: chunk.range.start.line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: chunk.range.end.line as u32,
                        character: 0,
                    },
                }),
            }));
        }
    } else {
        // Typst Hover (forward to tinymist)
        if let Some(typ_pos) = mapper.knot_to_typ_position(pos) {
            let mut tinymist_guard = state.tinymist.write().await;
            if let Some(proxy) = tinymist_guard.as_mut() {
                let params = serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": typ_pos
                });

                match proxy.send_request("textDocument/hover", params).await {
                    Ok(response) => {
                        if let Some(result) = response.get("result") {
                            if result.is_null() {
                                return Ok(None);
                            }

                            if let Ok(mut hover) = serde_json::from_value::<Hover>(result.clone()) {
                                // Map range back if present
                                if let Some(range) = hover.range {
                                    if let (Some(start), Some(end)) = (
                                        mapper.typ_to_knot_position(range.start),
                                        mapper.typ_to_knot_position(range.end),
                                    ) {
                                        hover.range = Some(Range { start, end });
                                    }
                                }
                                return Ok(Some(hover));
                            }
                        }
                    }
                    Err(_) => {
                        // Error logging handled by caller or proxy
                    }
                }
            }
        }
    }
    Ok(None)
}
