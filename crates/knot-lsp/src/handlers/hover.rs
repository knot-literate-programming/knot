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
        // If on fence line, show chunk metadata (not Typst hover)
        if mapper.is_position_on_fence(pos) {
            let doc = match Document::parse(text.clone()) {
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

                // Format Option<bool> values - show "default" if not set
                let eval_display = chunk.options.eval
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "default".to_string());
                let echo_display = chunk.options.echo
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "default".to_string());
                let cache_display = chunk.options.cache
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "default".to_string());

                content.push_str(&format!("- **Eval**: `{}`\n", eval_display));
                content.push_str(&format!("- **Echo**: `{}`\n", echo_display));
                content.push_str(&format!("- **Cache**: `{}`\n", cache_display));

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
            return Ok(None);
        }

        // Not on fence line, so we're in chunk content (option lines or R code)
        let line = pos.line as usize;
        let lines: Vec<&str> = text.lines().collect();

        // Check if we are on an option line (#|)
        if line < lines.len() && lines[line].trim_start().starts_with("#|") {
            return Ok(None);
        }

        // We are hovering over R code content -> Intelligent R Hover
        if let Some(token) = get_r_token_at_pos(&text, pos) {
            if !token.is_empty() {
                return Ok(get_r_help(state, uri, &token).await);
            }
        }

        return Ok(None);
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

fn get_r_token_at_pos(text: &str, pos: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    
    let line = lines[line_idx];
    let col = pos.character as usize;
    if col > line.len() {
        return None;
    }
    
    // Find the word boundaries around the cursor
    // We want to capture [a-zA-Z0-9_.$:]+
    
    let chars: Vec<char> = line.chars().collect();
    let mut start = col;
    let mut end = col;
    
    // Scan backwards
    while start > 0 {
        let c = chars[start - 1];
        if c.is_alphanumeric() || c == '_' || c == '.' {
            start -= 1;
        } else {
            break;
        }
    }
    
    // Scan forwards
    while end < chars.len() {
        let c = chars[end];
        if c.is_alphanumeric() || c == '_' || c == '.' {
            end += 1;
        } else {
            break;
        }
    }
    
    if start == end {
        return None;
    }
    
    Some(line[start..end].to_string())
}

async fn get_r_help(state: &ServerState, uri: &Url, token: &str) -> Option<Hover> {
    // Sync with cache if snapshot has changed (e.g., knot watch updated it)
    sync_cache_if_needed(state, uri).await;

    let output = {
        let mut managers = state.executors.write().await;
        if let Some(manager) = managers.get_mut(uri) {
            if let Ok(executor) = manager.get_executor("r") {
                // Robust R help query using a simpler approach:
                // 1. Write help to a temp file directly (avoids capture.output issues)
                // 2. Read back the file content
                let code = format!(
                    r#"{{
                        topic <- "{0}"
                        h <- utils::help(topic)
                        if (length(h) == 0) h <- utils::help(topic, try.all.packages = TRUE)
                        if (length(h) > 0) {{
                            tf <- tempfile()
                            tryCatch({{
                                rd <- utils:::.getHelpFile(h)
                                tools::Rd2txt(rd, out = tf, options = list(underline_titles = FALSE))
                                cat(readLines(tf, warn = FALSE), sep = "\n")
                            }}, error = function(e) {{
                                cat("Error rendering help:", as.character(e), "\n")
                            }}, finally = {{
                                if (file.exists(tf)) unlink(tf)
                            }})
                        }} else {{
                            cat("No help found for '{0}'\n")
                        }}
                    }}"#,
                    token.replace('"', "\\\"")
                );

                executor.execute_inline(&code).ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(output) = output {
         // Remove potential backticks from execute_inline formatting
        let clean_output = output.trim().trim_matches('`').trim();

        if !clean_output.is_empty() {
            // Wrap in Markdown code block
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```text\n{}\n```", clean_output),
                }),
                range: None,
            });
        }
    }
    None
}

/// Sync with cache if the snapshot has changed (e.g., from knot watch)
async fn sync_cache_if_needed(state: &ServerState, uri: &Url) {
    use knot_core::config::Config;
    use knot_core::get_cache_dir;
    use knot_core::cache::Cache;
    use std::path::Path;

    let path = match uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return,
    };

    let start_dir = path.parent().unwrap_or(Path::new("."));
    let (_config, project_root) = match Config::find_and_load(start_dir) {
        Ok(res) => res,
        Err(_) => return,
    };

    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
    let cache_dir = get_cache_dir(&project_root, file_stem);

    if let Ok(cache) = Cache::new(cache_dir.clone()) {
        if let Some(last_chunk) = cache.metadata.chunks.iter().max_by_key(|c| c.index) {
            let snapshot_hash = &last_chunk.hash;

            // Check if this snapshot is different from the last loaded one
            let should_reload = {
                let loaded_hashes = state.loaded_snapshot_hash.read().await;
                loaded_hashes.get(uri).map_or(true, |h| h != snapshot_hash)
            };

            if should_reload {
                let snapshot_path = cache_dir.join(format!("snapshot_{}.RData", snapshot_hash));
                if snapshot_path.exists() {
                    let success = {
                        let mut managers = state.executors.write().await;
                        if let Some(manager) = managers.get_mut(uri) {
                            if let Ok(executor) = manager.get_executor("r") {
                                executor.load_session(&snapshot_path).is_ok()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    if success {
                        state.loaded_snapshot_hash.write().await.insert(uri.clone(), snapshot_hash.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use crate::position_mapper::PositionMapper;

    async fn create_test_state(uri: &str, text: &str) -> (ServerState, Url) {
        let state = ServerState::new();
        let url = Url::parse(uri).unwrap();

        // Insert document
        {
            let mut docs = state.documents.write().await;
            docs.insert(url.clone(), text.to_string());
        }

        // Insert mapper (needs typ_content, use same text for simplicity in tests)
        let mapper = PositionMapper::new(text, text);
        {
            let mut mappers = state.mappers.read().await;
            mappers.insert(url.clone(), mapper);
        }

        (state, url)
    }

    fn create_hover_params(uri: &Url, line: u32, character: u32) -> HoverParams {
        HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        }
    }

    #[tokio::test]
    async fn test_hover_on_chunk_fence() {
        let text = r#"= Document

```{r my-chunk}
#| eval: true
#| echo: false
x <- 1:10
mean(x)
```

More text.
"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Hover on the opening fence (line 2: ```{r my-chunk})
        let params = create_hover_params(&uri, 2, 0);
        let result = handle_hover(&state, params).await.unwrap();

        assert!(result.is_some());
        let hover = result.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert!(content.value.contains("my-chunk"));
            assert!(content.value.contains("Language"));
            assert!(content.value.contains("r"));
            assert!(content.value.contains("Eval"));
            assert!(content.value.contains("true"));
            assert!(content.value.contains("Echo"));
            assert!(content.value.contains("false"));
        } else {
            panic!("Expected Markup hover contents");
        }
    }

    #[tokio::test]
    async fn test_hover_on_closing_fence() {
        let text = r#"```{r test}
x <- 1
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Hover on closing fence (line 2: ```)
        let params = create_hover_params(&uri, 2, 0);
        let result = handle_hover(&state, params).await.unwrap();

        assert!(result.is_some());
        let hover = result.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert!(content.value.contains("test"));
        } else {
            panic!("Expected Markup hover contents");
        }
    }

    #[tokio::test]
    async fn test_hover_outside_chunk() {
        let text = r#"= Document

```{r}
x <- 1
```

Regular text here.
"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Hover on regular text (line 6)
        let params = create_hover_params(&uri, 6, 0);
        let result = handle_hover(&state, params).await.unwrap();

        // Should return None (would forward to tinymist if available)
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_hover_unnamed_chunk() {
        let text = r#"```{r}
#| cache: true
x <- 1
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Hover on opening fence
        let params = create_hover_params(&uri, 0, 0);
        let result = handle_hover(&state, params).await.unwrap();

        assert!(result.is_some());
        let hover = result.unwrap();

        if let HoverContents::Markup(content) = hover.contents {
            assert!(content.value.contains("unnamed"));
            assert!(content.value.contains("Cache"));
            assert!(content.value.contains("true"));
        } else {
            panic!("Expected Markup hover contents");
        }
    }

    #[tokio::test]
    async fn test_hover_document_not_found() {
        let state = ServerState::new();
        let uri = Url::parse("file:///nonexistent.knot").unwrap();

        let params = create_hover_params(&uri, 0, 0);
        let result = handle_hover(&state, params).await.unwrap();

        // Should return None when document not found
        assert!(result.is_none());
    }
}