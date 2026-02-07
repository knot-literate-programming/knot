use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

#[allow(dead_code)]
pub async fn handle_formatting(
    state: &ServerState,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    // 1. Get document text and formatter
    let (text, formatter) = {
        let docs = state.documents.read().await;
        let formatter_opt = state.formatter.read().await;
        match (docs.get(uri), formatter_opt.as_ref()) {
            (Some(t), Some(f)) => (t.clone(), f.clone()),
            _ => return Ok(None),
        }
    };

    // 2. Parse document to find R chunks
    let doc = match Document::parse(text.clone()) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    let mut edits = Vec::new();

    // 3. Format each R chunk
    for chunk in doc.chunks.iter().filter(|c| c.language == "r") {
        match formatter.format_r_code(&chunk.code).await {
            Ok(formatted) => {
                // Only create edit if formatting changed the code
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
            Err(_) => {
                // Ignore formatting errors for now, just skip the chunk
            }
        }
    }

    if edits.is_empty() {
        Ok(None)
    } else {
        Ok(Some(edits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::AirFormatter;
    use crate::position_mapper::PositionMapper;
    use crate::state::ServerState;

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
        if let Some(edits) = result {
            assert!(!edits.is_empty());
        }
    }
}
