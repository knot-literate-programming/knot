use super::get_token_at_pos;
use crate::state::ServerState;
use knot_core::parser::parse_document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_hover(state: &ServerState, params: HoverParams) -> Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;

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
        // Skip fence lines
        if line == chunk.range.start.line || line == chunk.range.end.line {
            let name = chunk.name.as_deref().unwrap_or("unnamed");
            let content = format!(
                "### Knot Chunk: `{}`\n- **Language**: `{}`",
                name, chunk.language
            );
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: None,
            }));
        }

        // Skip option lines
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() && lines[line].trim_start().starts_with("#|") {
            return Ok(None);
        }

        if let Some(token) = get_token_at_pos(&text, pos, &chunk.language, true) {
            if chunk.language == "r" {
                return Ok(get_r_help(state, uri, &token).await);
            } else if chunk.language == "python" {
                return Ok(get_python_help(state, uri, &token).await);
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
        if let Some(token) = get_token_at_pos(&text, pos, &inline.language, true) {
            if inline.language == "r" {
                return Ok(get_r_help(state, uri, &token).await);
            } else if inline.language == "python" {
                return Ok(get_python_help(state, uri, &token).await);
            }
        }
        return Ok(None);
    }

    // 3. Forward to tinymist for regular Typst content
    if let Some(typ_pos) = mapper.knot_to_typ_position(pos) {
        let mut tinymist_guard = state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let virtual_uri = crate::transform::to_virtual_uri(uri);
            let params =
                serde_json::json!({ "textDocument": { "uri": virtual_uri }, "position": typ_pos });
            if let Ok(response) = proxy.send_request("textDocument/hover", params).await {
                if let Some(result) = response.get("result") {
                    if let Ok(mut hover) = serde_json::from_value::<Hover>(result.clone()) {
                        if let Some(range) = &mut hover.range {
                            if let (Some(s), Some(e)) = (
                                mapper.typ_to_knot_position(range.start),
                                mapper.typ_to_knot_position(range.end),
                            ) {
                                *range = Range { start: s, end: e };
                            }
                        }
                        return Ok(Some(hover));
                    }
                }
            }
        }
    }

    Ok(None)
}

pub async fn get_python_help(state: &ServerState, uri: &Url, token: &str) -> Option<Hover> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("python").ok()?;

    let code = format!("print(get_hover(\"{}\"))", token.replace('"', "\\\""));
    let out = executor.query(&code).ok()?;

    if out.trim().is_empty() || out.contains("No help found") {
        return None;
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```text\n{}\n```", out.trim()),
        }),
        range: None,
    })
}

pub async fn get_r_help(state: &ServerState, uri: &Url, token: &str) -> Option<Hover> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("r").ok()?;

    // Use the clean helper function
    let code = format!("cat(get_hover('{}'))", token.replace('\'', "\\'"));
    let out = executor.query(&code).ok()?;

    if out.trim().is_empty() {
        return None;
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```text\n{}\n```", out.trim()),
        }),
        range: None,
    })
}
