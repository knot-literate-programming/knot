use crate::state::ServerState;
use knot_core::parser::parse_document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_completion(state: &ServerState, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;

    let (text, mapper) = {
        let docs = state.documents.read().await;
        let mappers = state.mappers.read().await;
        match (docs.get(uri), mappers.get(uri)) {
            (Some(t), Some(m)) => (t.clone(), m.clone()),
            _ => return Ok(None),
        }
    };

    let doc = parse_document(&text);
    let line = pos.line as usize;
    let current_chunk = doc.chunks.iter().find(|c| {
        line >= c.range.start.line && line <= c.range.end.line
    });

    if let Some(chunk) = current_chunk {
        // Skip fences
        if line == chunk.range.start.line || line == chunk.range.end.line { return Ok(None); }

        let lines: Vec<&str> = text.lines().collect();
        let trimmed = lines.get(line).map(|l| l.trim_start()).unwrap_or("");

        if trimmed.starts_with("#|") {
            let options = vec!["eval", "echo", "output", "cache", "fig-width", "fig-height", "dpi", "constant"];
            let items = options.into_iter().map(|o| CompletionItem {
                label: o.to_string(),
                kind: Some(CompletionItemKind::FIELD),
                insert_text: Some(format!("{}: ", o)),
                ..Default::default()
            }).collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }

        if let Some(token) = get_token_at_pos(&text, pos, &chunk.language) {
            if chunk.language == "r" {
                return Ok(get_r_completion(state, uri, &token).await);
            } else if chunk.language == "python" {
                return Ok(get_python_completion(state, uri, &token).await);
            }
        }
    } else if let Some(typ_pos) = mapper.knot_to_typ_position(pos) {
        // Forward to tinymist
        let mut tinymist_guard = state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let mut typ_params = serde_json::to_value(&params).unwrap_or_default();
            if let Some(pos_obj) = typ_params.pointer_mut("/position") {
                *pos_obj = serde_json::to_value(typ_pos).unwrap_or_default();
            }
            if let Ok(resp) = proxy.send_request("textDocument/completion", typ_params).await {
                if let Some(res) = resp.get("result") {
                    if let Ok(mut comp) = serde_json::from_value::<CompletionResponse>(res.clone()) {
                        match &mut comp {
                            CompletionResponse::Array(items) => for i in items { map_item(i, &mapper); }
                            CompletionResponse::List(l) => for i in &mut l.items { map_item(i, &mapper); }
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
            CompletionTextEdit::Edit(e) => if let (Some(s), Some(end)) = (mapper.typ_to_knot_position(e.range.start), mapper.typ_to_knot_position(e.range.end)) {
                e.range = Range { start: s, end };
            }
            CompletionTextEdit::InsertAndReplace(e) => if let (Some(s1), Some(e1), Some(s2), Some(e2)) = (mapper.typ_to_knot_position(e.insert.start), mapper.typ_to_knot_position(e.insert.end), mapper.typ_to_knot_position(e.replace.start), mapper.typ_to_knot_position(e.replace.end)) {
                e.insert = Range { start: s1, end: e1 }; e.replace = Range { start: s2, end: e2 };
            }
        }
    }
}

fn get_token_at_pos(text: &str, pos: Position, lang: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    let chars: Vec<char> = line.chars().collect();
    if col > chars.len() { return None; }
    let mut start = col;
    while start > 0 && is_id_char(chars[start-1], lang) { start -= 1; }
    Some(line[start..col].to_string())
}

fn is_id_char(c: char, lang: &str) -> bool {
    if lang == "r" { c.is_alphanumeric() || c == '_' || c == '.' || c == '$' || c == ':' }
    else { c.is_alphanumeric() || c == '_' || c == '.' }
}

async fn get_python_completion(state: &ServerState, uri: &Url, token: &str) -> Option<CompletionResponse> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("python").ok()?;
    let code = format!(
        "import builtins\ntoken = '{}'\nif '.' in token:\n    parts = token.split('.')\n    obj = globals().get(parts[0])\n    if obj is not None:\n        for part in parts[1:-1]:\n            obj = getattr(obj, part, None)\n            if obj is None:\n                break\n        if obj is not None:\n            prefix = parts[-1]\n            candidates = [x for x in dir(obj) if not x.startswith('_') and x.startswith(prefix)]\n            print('\\n'.join(candidates[:50]))\n        else:\n            print('')\n    else:\n        print('')\nelse:\n    candidates = list(globals().keys()) + dir(builtins)\n    print('\\n'.join([c for c in candidates if c.startswith(token)][:50]))\n",
        token.replace('\'', "\\'").replace('"', "\\\"")
    );
    let out = executor.query(&code).ok()?;
    let items = out.lines().filter(|l| !l.trim().is_empty()).map(|l| {
        let kind = if token.contains('.') { CompletionItemKind::METHOD } else { CompletionItemKind::VARIABLE };
        CompletionItem { label: l.trim().to_string(), kind: Some(kind), ..Default::default() }
    }).collect::<Vec<_>>();
    if items.is_empty() { None } else { Some(CompletionResponse::Array(items)) }
}

async fn get_r_completion(state: &ServerState, uri: &Url, token: &str) -> Option<CompletionResponse> {
    let mut managers = state.executors.write().await;
    let manager = managers.get_mut(uri)?;
    let executor = manager.get_executor("r").ok()?;
    let code = if token.contains('$') {
        let parts: Vec<&str> = token.split('$').collect();
        format!("{{ obj<-get('{0}'); n<-names(obj); cat(paste(n[startsWith(n, '{1}')], collapse='\\n')) }}", parts[0], parts.get(1).unwrap_or(&""))
    } else {
        format!("{{ cat(paste(utils::apropos('^{}'), collapse='\\n')) }}", token)
    };
    let out = executor.query(&code).ok()?;
    let items = out.lines().filter(|l| !l.trim().is_empty()).map(|l| CompletionItem { label: l.trim().to_string(), kind: Some(if token.contains('$') { CompletionItemKind::FIELD } else { CompletionItemKind::FUNCTION }), ..Default::default() }).collect::<Vec<_>>();
    if items.is_empty() { None } else { Some(CompletionResponse::Array(items)) }
}