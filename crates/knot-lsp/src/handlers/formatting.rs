use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_formatting(
    state: &ServerState,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    // 1. Get document text and formatter
    let (text, _formatter) = {
        let docs = state.documents.read().await;
        let formatter_opt = state.formatter.read().await;
        match (docs.get(uri), formatter_opt.as_ref()) {
            (Some(t), Some(f)) => (t.clone(), Some(f.clone())),
            (Some(t), None) => (t.clone(), None),
            _ => return Ok(None),
        }
    };

    // 2. Parse document to find all chunks
    let doc = match Document::parse(text.clone()) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    let mut edits = Vec::new();

    // 3. Format each chunk (structurally)
    for chunk in &doc.chunks {
        // Normalization via Core (header, options)
        // TODO: Integrate Air/Ruff here for code formatting
        let formatted = chunk.format();

        // Only create edit if formatting changed the chunk
        let original_chunk = &text[chunk.start_byte..chunk.end_byte];
        
        if formatted != original_chunk {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: chunk.range.start.line as u32,
                        character: chunk.range.start.column as u32,
                    },
                    end: Position {
                        line: chunk.range.end.line as u32,
                        character: chunk.range.end.column as u32,
                    },
                },
                new_text: formatted,
            });
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
    async fn test_formatting_structural_normalization() {
        let text = r#"```{r   my-chunk   }
#| eval:true
#|cache:  false
print(42)
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;
        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        assert!(result.is_some());
        let edits = result.unwrap();
        let new_text = &edits[0].new_text;
        
        // Header should be normalized
        assert!(new_text.contains("```{r my-chunk}"));
        // Options should be normalized
        assert!(new_text.contains("#| eval: true"));
        assert!(new_text.contains("#| cache: false"));
    }
}
