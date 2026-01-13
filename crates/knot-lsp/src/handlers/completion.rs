use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::state::ServerState;
use crate::position_mapper::PositionMapper;

pub async fn handle_completion(state: &ServerState, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;

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
        // Knot (R chunk) Completion
        let lines: Vec<&str> = text.lines().collect();
        let line_idx = pos.line as usize;
        
        if line_idx < lines.len() {
            let line_text = lines[line_idx];
            let trimmed = line_text.trim_start();
            
            // If we are on a line starting with #| suggest chunk options
            if trimmed.starts_with("#|") {
                let options = vec![
                    ("eval", "Evaluate the code chunk (true/false)"),
                    ("echo", "Display the code in the output (true/false)"),
                    ("output", "Display the results in the output (true/false)"),
                    ("cache", "Cache the results of the chunk (true/false)"),
                    ("fig-width", "Width of the figure in inches"),
                    ("fig-height", "Height of the figure in inches"),
                    ("dpi", "DPI for the figure"),
                ];

                let items = options.into_iter().map(|(name, detail)| {
                    CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(detail.to_string()),
                        insert_text: Some(format!("{}: ", name)),
                        ..Default::default()
                    }
                }).collect();

                return Ok(Some(CompletionResponse::Array(items)));
            }
        }
    } else {
        // Typst Completion (forward to tinymist)
        if let Some(typ_pos) = mapper.knot_to_typ_position(pos) {
            let mut tinymist_guard = state.tinymist.write().await;
            if let Some(proxy) = tinymist_guard.as_mut() {
                let mut typ_params = serde_json::to_value(&params).unwrap_or_default();
                if let Some(obj) = typ_params.as_object_mut() {
                    if let Some(pos_obj) = obj.get_mut("position") {
                        *pos_obj = serde_json::to_value(typ_pos).unwrap_or_default();
                    }
                }

                match proxy.send_request("textDocument/completion", typ_params).await {
                    Ok(response) => {
                        if let Some(result) = response.get("result") {
                            if result.is_null() {
                                return Ok(None);
                            }
                            
                            if let Ok(mut completion) = serde_json::from_value::<CompletionResponse>(result.clone()) {
                                // Map ranges in items back if present
                                match &mut completion {
                                    CompletionResponse::Array(items) => {
                                        for item in items {
                                            map_completion_item(item, &mapper);
                                        }
                                    }
                                    CompletionResponse::List(list) => {
                                        for item in &mut list.items {
                                            map_completion_item(item, &mapper);
                                        }
                                    }
                                }
                                return Ok(Some(completion));
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

fn map_completion_item(item: &mut CompletionItem, mapper: &PositionMapper) {
    if let Some(edit) = &mut item.text_edit {
        match edit {
            CompletionTextEdit::Edit(text_edit) => {
                if let (Some(start), Some(end)) = (
                    mapper.typ_to_knot_position(text_edit.range.start),
                    mapper.typ_to_knot_position(text_edit.range.end)
                ) {
                    text_edit.range.start = start;
                    text_edit.range.end = end;
                }
            }
            CompletionTextEdit::InsertAndReplace(iar) => {
                if let (Some(insert_start), Some(insert_end), Some(replace_start), Some(replace_end)) = (
                    mapper.typ_to_knot_position(iar.insert.start),
                    mapper.typ_to_knot_position(iar.insert.end),
                    mapper.typ_to_knot_position(iar.replace.start),
                    mapper.typ_to_knot_position(iar.replace.end)
                ) {
                    iar.insert.start = insert_start;
                    iar.insert.end = insert_end;
                    iar.replace.start = replace_start;
                    iar.replace.end = replace_end;
                }
            }
        }
    }
    
    if let Some(additional_edits) = &mut item.additional_text_edits {
        for edit in additional_edits {
            if let (Some(start), Some(end)) = (
                mapper.typ_to_knot_position(edit.range.start),
                mapper.typ_to_knot_position(edit.range.end)
            ) {
                edit.range.start = start;
                edit.range.end = end;
            }
        }
    }
}
