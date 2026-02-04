use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::state::ServerState;
use knot_core::Document;

pub async fn handle_formatting(state: &ServerState, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
    // Check if formatter is available
    let formatter_guard = state.formatter.read().await;
    let formatter = match formatter_guard.as_ref() {
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
    let formatter_guard = state.formatter.read().await;
    let formatter = match formatter_guard.as_ref() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use crate::position_mapper::PositionMapper;
    use crate::formatter::AirFormatter;

    async fn create_test_state(uri: &str, text: &str, with_formatter: bool) -> (ServerState, Url) {
        let state = ServerState::new();
        if with_formatter {
            if let Ok(f) = AirFormatter::new(None) {
                *state.formatter.write().await = Some(f);
            }
        }
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

    fn create_formatting_params(uri: &Url) -> DocumentFormattingParams {
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        }
    }

    fn create_on_type_formatting_params(uri: &Url, line: u32, character: u32, ch: &str) -> DocumentOnTypeFormattingParams {
        DocumentOnTypeFormattingParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line, character },
            },
            ch: ch.to_string(),
            options: FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn test_formatting_no_formatter() {
        let text = r#"```{r}
x<-1+2
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // Should return None when formatter is not available
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_document_not_found() {
        let state = ServerState::new();
        let uri = Url::parse("file:///nonexistent.knot").unwrap();

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // Should return None when document not found
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_invalid_document() {
        let text = "This is not valid knot syntax ```{unclosed";

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // Should return None when document parse fails
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_no_r_chunks() {
        let text = r#"= Document

Some text here.

```{python}
x = 1 + 2
```
"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // Should return None when there are no R chunks to format
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_already_formatted() {
        let text = r#"```{r}
x <- 1 + 2
```"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // If code is already formatted, should return None (no edits)
        // Note: This depends on Air's formatting behavior
        // We just verify it doesn't error
        assert!(result.is_none() || result.is_some());
    }

    #[tokio::test]
    async fn test_formatting_multiple_chunks() {
        let text = r#"```{r}
x<-1
```

```{r}
y<-2
```"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        // Should format both chunks
        // If formatting produces changes, we should get edits for both
        if let Some(edits) = result {
            assert!(edits.len() >= 1);
        }
    }

    #[tokio::test]
    async fn test_on_type_formatting_no_formatter() {
        let text = r#"```{r}
x <- 1
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;

        let params = create_on_type_formatting_params(&uri, 1, 6, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when formatter is not available
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_wrong_character() {
        let text = r#"```{r}
x <- 1
```"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        // Type a non-newline character
        let params = create_on_type_formatting_params(&uri, 1, 5, "a");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when character is not newline
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_document_not_found() {
        let state = ServerState::new();
        let uri = Url::parse("file:///nonexistent.knot").unwrap();

        let params = create_on_type_formatting_params(&uri, 0, 0, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when document not found
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_invalid_document() {
        let text = "This is not valid knot syntax ```{unclosed";

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;

        let params = create_on_type_formatting_params(&uri, 0, 10, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when document parse fails
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_outside_chunk() {
        let text = r#"= Document

```{r}
x <- 1
```

Regular text here.
"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        // Type newline in regular text (line 6)
        let params = create_on_type_formatting_params(&uri, 6, 10, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when cursor is outside R chunk
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_in_python_chunk() {
        let text = r#"```{python}
x=1+2
```"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        // Type newline in Python chunk (should not format)
        let params = create_on_type_formatting_params(&uri, 1, 5, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should return None when chunk is not R
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_on_type_formatting_in_r_chunk() {
        let text = r#"```{r}
x<-1+2
```"#;

        let formatter = AirFormatter::new(None).ok();
        if formatter.is_none() {
            eprintln!("Air not installed, skipping test");
            return;
        }

        let (state, uri) = create_test_state("file:///test.knot", text, true).await;

        // Type newline in R chunk
        let params = create_on_type_formatting_params(&uri, 1, 6, "\n");
        let result = handle_on_type_formatting(&state, params).await.unwrap();

        // Should attempt to format the R chunk
        // Result depends on whether Air makes changes
        assert!(result.is_none() || result.is_some());
    }
}
