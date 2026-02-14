use super::get_token_at_pos;
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
        let trimmed = lines.get(line).map(|l| l.trim_start()).unwrap_or("");

        if trimmed.starts_with("#|") {
            let options = vec![
                "eval",
                "echo",
                "output",
                "cache",
                "fig-width",
                "fig-height",
                "dpi",
                "constant",
            ];
            let items = options
                .into_iter()
                .map(|o| CompletionItem {
                    label: o.to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    insert_text: Some(format!("{}: ", o)),
                    ..Default::default()
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

            if let Some(doc_obj) = typ_params.pointer_mut("/textDocument") {
                if let Some(uri_val) = doc_obj.get_mut("uri") {
                    *uri_val = serde_json::to_value(virtual_uri).unwrap_or_default();
                }
            }

            if let Some(pos_obj) = typ_params.pointer_mut("/position") {
                *pos_obj = serde_json::to_value(typ_pos).unwrap_or_default();
            }

            if let Ok(resp) = proxy
                .send_request("textDocument/completion", typ_params)
                .await
            {
                if let Some(res) = resp.get("result") {
                    if let Ok(mut comp) = serde_json::from_value::<CompletionResponse>(res.clone())
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
