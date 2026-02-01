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

        // Insert mapper
        let mapper = PositionMapper::new(text, text);
        {
            let mut mappers = state.mappers.write().await;
            mappers.insert(url.clone(), mapper);
        }

        (state, url)
    }

    fn create_completion_params(uri: &Url, line: u32, character: u32) -> CompletionParams {
        CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        }
    }

    #[tokio::test]
    async fn test_completion_on_chunk_option_line() {
        let text = r#"```{r}
#|
x <- 1
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Cursor on line with #| (line 1, at position 2, right after #|)
        let params = create_completion_params(&uri, 1, 2);
        let result = handle_completion(&state, params).await.unwrap();

        assert!(result.is_some());

        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.len() > 0);

            // Check that we have expected options
            let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
            assert!(labels.contains(&"eval".to_string()));
            assert!(labels.contains(&"echo".to_string()));
            assert!(labels.contains(&"output".to_string()));
            assert!(labels.contains(&"cache".to_string()));
            assert!(labels.contains(&"fig-width".to_string()));
            assert!(labels.contains(&"fig-height".to_string()));
            assert!(labels.contains(&"dpi".to_string()));

            // Check that items have proper details
            let eval_item = items.iter().find(|i| i.label == "eval").unwrap();
            assert!(eval_item.detail.is_some());
            assert!(eval_item.detail.as_ref().unwrap().contains("Evaluate"));
            assert_eq!(eval_item.kind, Some(CompletionItemKind::FIELD));

            // Check insert text format
            assert_eq!(eval_item.insert_text, Some("eval: ".to_string()));
        } else {
            panic!("Expected CompletionResponse::Array");
        }
    }

    #[tokio::test]
    async fn test_completion_in_chunk_code() {
        let text = r#"```{r}
x <- 1:10
mean(x)
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Cursor on R code (line 1)
        let params = create_completion_params(&uri, 1, 5);
        let result = handle_completion(&state, params).await.unwrap();

        // Should return None to allow R LSP to handle
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_completion_on_partial_option_line() {
        let text = r#"```{r}
#| ev
x <- 1
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Cursor on line with partial option (line 1)
        let params = create_completion_params(&uri, 1, 5);
        let result = handle_completion(&state, params).await.unwrap();

        assert!(result.is_some());

        // Should still return all options (filtering is done by the editor)
        if let Some(CompletionResponse::Array(items)) = result {
            assert!(items.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_completion_outside_chunk() {
        let text = r#"= Document

```{r}
x <- 1
```

Regular text here.
"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Cursor on regular text (line 6)
        let params = create_completion_params(&uri, 6, 5);
        let result = handle_completion(&state, params).await.unwrap();

        // Should return None (would forward to tinymist if available)
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_completion_chunk_option_at_start() {
        let text = r#"```{r}
#|
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        // Cursor right after #| (line 1, character 2)
        let params = create_completion_params(&uri, 1, 2);
        let result = handle_completion(&state, params).await.unwrap();

        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_completion_document_not_found() {
        let state = ServerState::new(None);
        let uri = Url::parse("file:///nonexistent.knot").unwrap();

        let params = create_completion_params(&uri, 0, 0);
        let result = handle_completion(&state, params).await.unwrap();

        // Should return None when document not found
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_completion_option_details() {
        let text = r#"```{r}
#|
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text).await;

        let params = create_completion_params(&uri, 1, 3);
        let result = handle_completion(&state, params).await.unwrap();

        if let Some(CompletionResponse::Array(items)) = result {
            // Check each option has proper metadata
            for item in items {
                assert!(item.detail.is_some(), "Option {} should have detail", item.label);
                assert_eq!(item.kind, Some(CompletionItemKind::FIELD));
                assert!(item.insert_text.is_some());
                assert!(item.insert_text.as_ref().unwrap().ends_with(": "));
            }
        }
    }
}
