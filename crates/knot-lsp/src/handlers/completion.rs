use super::get_token_at_pos;
use crate::lsp_methods::text_document as lsp;
use crate::state::ServerState;
use knot_core::parser::parse_document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_completion(
    state: &ServerState,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;

    let (text, mapper) = {
        let docs = state.documents.read().await;
        match docs.get(uri) {
            Some(doc) => (doc.text.clone(), doc.mapper.clone()),
            _ => return Ok(None),
        }
    };

    let doc = parse_document(&text);
    let line = pos.line as usize;

    // 1. Check if we are in a regular code chunk
    let current_chunk = doc
        .chunks
        .iter()
        .find(|c| line >= c.range.start.line && line <= c.range.end.line);

    if let Some(chunk) = current_chunk {
        if line == chunk.range.start.line || line == chunk.range.end.line {
            return Ok(None);
        }

        let lines: Vec<&str> = text.lines().collect();
        let line_text = lines.get(line).unwrap_or(&"");
        
        // Check if the current position is on an option line (#|)
        let prefix_part = if (pos.character as usize) <= line_text.encode_utf16().count() {
            let mut utf16_count = 0;
            let mut byte_idx = 0;
            for c in line_text.chars() {
                if utf16_count >= pos.character as usize { break; }
                utf16_count += c.len_utf16();
                byte_idx += c.len_utf8();
            }
            &line_text[..byte_idx]
        } else {
            line_text
        };

        if prefix_part.trim_start().starts_with("#|") {
            // Check if we are after a colon to suggest values
            if let Some(colon_pos) = prefix_part.find(':') {
                let option_name = prefix_part["#|".len()..colon_pos].trim();
                let values = match option_name {
                    "show" => vec!["both", "code", "output", "none"],
                    "layout" => vec!["horizontal", "vertical"],
                    "fig-format" => vec!["svg", "png"],
                    "warnings-visibility" => vec!["below", "inline", "none"],
                    "eval" | "cache" => vec!["true", "false"],
                    _ => vec![],
                };

                if !values.is_empty() {
                    // Detect if we need to insert a space (if the user typed ":" but not " ")
                    let needs_space = !prefix_part.ends_with(": ") && prefix_part.ends_with(':');
                    
                    let items = values
                        .into_iter()
                        .map(|v| CompletionItem {
                            label: v.to_string(),
                            kind: Some(CompletionItemKind::ENUM_MEMBER),
                            // Insert a space if needed
                            insert_text: Some(if needs_space { format!(" {}", v) } else { v.to_string() }),
                            ..Default::default()
                        })
                        .collect();
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }

            // Otherwise suggest option names
            let metadata = knot_core::parser::ChunkOptions::option_metadata();
            let items = metadata
                .into_iter()
                .map(|m| {
                    let serde_name = m.serde_name();
                    CompletionItem {
                        label: serde_name.clone(),
                        kind: Some(CompletionItemKind::FIELD),
                        insert_text: Some(format!("{}: ", serde_name)),
                        documentation: Some(Documentation::String(m.doc.to_string())),
                        detail: Some(format!("Default: {}", m.default_value)),
                        // Trigger completion again immediately after inserting the name and ":"
                        // to show the possible values.
                        command: Some(Command {
                            title: "triggerSuggest".to_string(),
                            command: "editor.action.triggerSuggest".to_string(),
                            arguments: None,
                        }),
                        ..Default::default()
                    }
                })
                .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }

        if let Some(token) = get_token_at_pos(&text, pos, &chunk.language, false) {
            if chunk.language == "r" {
                return Ok(get_r_completion(state, uri, &token).await);
            } else if chunk.language == "python" {
                return Ok(get_python_completion(state, uri, &token).await);
            }
        }
        return Ok(None);
    }

    // 2. Check if we are in an inline expression
    let byte_offset = mapper.offset_at_position(pos);
    let current_inline = doc
        .inline_exprs
        .iter()
        .find(|i| byte_offset >= i.start && byte_offset < i.end);

    if let Some(inline) = current_inline {
        if let Some(token) = get_token_at_pos(&text, pos, &inline.language, false) {
            if inline.language == "r" {
                return Ok(get_r_completion(state, uri, &token).await);
            } else if inline.language == "python" {
                return Ok(get_python_completion(state, uri, &token).await);
            }
        }
        return Ok(None);
    }

    // 3. Forward to tinymist for regular Typst content
    if let Some(typ_pos) = mapper.knot_to_typ_position(pos) {
        let mut tinymist_guard = state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let mut typ_params = serde_json::to_value(&params).unwrap_or_default();
            let virtual_uri = crate::transform::to_virtual_uri(uri);

            if let Some(doc_obj) = typ_params.pointer_mut("/textDocument")
                && let Some(uri_val) = doc_obj.get_mut("uri")
            {
                *uri_val = serde_json::to_value(virtual_uri).unwrap_or_default();
            }

            if let Some(pos_obj) = typ_params.pointer_mut("/position") {
                *pos_obj = serde_json::to_value(typ_pos).unwrap_or_default();
            }

            if let Ok(resp) = proxy.send_request(lsp::COMPLETION, typ_params).await
                && let Some(res) = resp.get("result")
                && let Ok(mut comp) = serde_json::from_value::<CompletionResponse>(res.clone())
            {
                match &mut comp {
                    CompletionResponse::Array(items) => {
                        for i in items {
                            map_item(i, &mapper);
                        }
                    }
                    CompletionResponse::List(l) => {
                        for i in &mut l.items {
                            map_item(i, &mapper);
                        }
                    }
                }
                return Ok(Some(comp));
            }
        }
    }
    Ok(None)
}

fn map_item(item: &mut CompletionItem, mapper: &crate::position_mapper::PositionMapper) {
    if let Some(edit) = &mut item.text_edit {
        match edit {
            CompletionTextEdit::Edit(e) => {
                if let (Some(s), Some(end)) = (
                    mapper.typ_to_knot_position(e.range.start),
                    mapper.typ_to_knot_position(e.range.end),
                ) {
                    e.range = Range { start: s, end };
                }
            }
            CompletionTextEdit::InsertAndReplace(e) => {
                if let (Some(s1), Some(e1), Some(s2), Some(e2)) = (
                    mapper.typ_to_knot_position(e.insert.start),
                    mapper.typ_to_knot_position(e.insert.end),
                    mapper.typ_to_knot_position(e.replace.start),
                    mapper.typ_to_knot_position(e.replace.end),
                ) {
                    e.insert = Range { start: s1, end: e1 };
                    e.replace = Range { start: s2, end: e2 };
                }
            }
        }
    }
}

async fn get_python_completion(
    state: &ServerState,
    uri: &Url,
    token: &str,
) -> Option<CompletionResponse> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("python").ok()?;

    let code = format!("print(get_completions(\"{}\"))", token.replace('"', "\\\""));
    let out = executor.query(&code).ok()?;

    let items = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let kind = if token.contains('.') {
                CompletionItemKind::METHOD
            } else {
                CompletionItemKind::VARIABLE
            };
            CompletionItem {
                label: l.trim().to_string(),
                kind: Some(kind),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        None
    } else {
        Some(CompletionResponse::Array(items))
    }
}

async fn get_r_completion(
    state: &ServerState,
    uri: &Url,
    token: &str,
) -> Option<CompletionResponse> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("r").ok()?;

    // Use the clean helper function
    let code = format!("cat(get_completions('{}'))", token.replace('\'', "\\'"));
    let out = executor.query(&code).ok()?;

    let items = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let kind = if token.contains('$') {
                CompletionItemKind::FIELD
            } else {
                CompletionItemKind::FUNCTION
            };
            CompletionItem {
                label: l.trim().to_string(),
                kind: Some(kind),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        None
    } else {
        Some(CompletionResponse::Array(items))
    }
}
