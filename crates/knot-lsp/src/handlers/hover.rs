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
        // Check if we're hovering specifically over the chunk fence (header or closing)
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
            // Only show chunk metadata if hovering over the fence lines
            // (chunk.range.start.line is the ```{r line, chunk.range.end.line is the closing ```)
            if line == chunk.range.start.line || line == chunk.range.end.line {
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
            // Otherwise (we're hovering over R code content), return None
            // to allow VSCode to delegate to R language server if installed
            return Ok(None);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use crate::position_mapper::PositionMapper;

    async fn create_test_state(uri: &str, text: &str) -> (ServerState, Url) {
        let state = ServerState::new(None);
        let url = Url::parse(uri).unwrap();

        // Insert document
        {
            let mut docs = state.documents.write().await;
            docs.insert(url.clone(), text.to_string());
        }

        // Insert mapper (needs typ_content, use same text for simplicity in tests)
        let mapper = PositionMapper::new(text, text);
        {
            let mut mappers = state.mappers.write().await;
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
    async fn test_hover_in_chunk_code() {
        let text = r#"```{r}
x <- 1:10
mean(x)
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Hover inside R code (line 1: x <- 1:10)
        let params = create_hover_params(&uri, 1, 5);
        let result = handle_hover(&state, params).await.unwrap();

        // Should return None to allow R LSP to handle
        assert!(result.is_none());
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
        let state = ServerState::new(None);
        let uri = Url::parse("file:///nonexistent.knot").unwrap();

        let params = create_hover_params(&uri, 0, 0);
        let result = handle_hover(&state, params).await.unwrap();

        // Should return None when document not found
        assert!(result.is_none());
    }
}
