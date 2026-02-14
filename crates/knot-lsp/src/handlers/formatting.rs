use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_formatting(
    state: &ServerState,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    let (text, _formatter) = {
        let docs = state.documents.read().await;
        let formatter_opt = state.formatter.read().await;
        match (docs.get(uri), formatter_opt.as_ref()) {
            (Some(doc), Some(f)) => (doc.text.clone(), Some(f.clone())),
            (Some(doc), None) => (doc.text.clone(), None),
            _ => return Ok(None),
        }
    };

    let doc = match Document::parse(text.clone()) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    let mut edits = Vec::new();

    for chunk in &doc.chunks {
        let formatted_code =
            knot_core::compiler::formatters::format_code(&chunk.code, &chunk.language).ok();
        let formatted = chunk.format(formatted_code.as_deref());
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

/// Format a single chunk at the given position
pub async fn handle_format_chunk(
    state: &ServerState,
    uri: &Url,
    pos: Position,
) -> Result<Option<WorkspaceEdit>> {
    // 1. Get document text
    let text = {
        let docs = state.documents.read().await;
        match docs.get(uri) {
            Some(doc) => doc.text.clone(),
            _ => return Ok(None),
        }
    };

    // 2. Parse document to find the chunk under cursor
    let doc = match Document::parse(text.clone()) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    let line = pos.line as usize;
    let target_chunk = doc
        .chunks
        .iter()
        .find(|c| line >= c.range.start.line && line <= c.range.end.line);

    if let Some(chunk) = target_chunk {
        // 3. Format the chunk
        let formatted_code =
            knot_core::compiler::formatters::format_code(&chunk.code, &chunk.language).ok();
        let formatted = chunk.format(formatted_code.as_deref());

        let original_chunk = &text[chunk.start_byte..chunk.end_byte];

        if formatted != original_chunk {
            let edit = TextEdit {
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
            };

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), vec![edit]);

            return Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }));
        }
    }

    Ok(None)
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

        let mapper = PositionMapper::new(text, text);
        {
            let mut docs = state.documents.write().await;
            docs.insert(url.clone(), crate::state::DocumentState {
                text: text.to_string(),
                version: 1,
                mapper,
                opened_in_tinymist: false,
                knot_diagnostics: Vec::new(),
                tinymist_diagnostics: Vec::new(),
            });
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

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_invalid_document() {
        let text = "This is not valid knot syntax ```{unclosed";

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;

        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_formatting_structural_normalization() {
        // Tests two normalization behaviors:
        // 1. Chunk header: extra spaces around name are removed
        // 2. Option lines: missing space after `#|` is added (e.g. `#|cache:` → `#| cache:`)
        // Note: `eval:true` (no space after colon) is not valid YAML key-value syntax,
        // so we use `eval: false` (non-default, valid YAML) to verify option preservation.
        let text = r#"```{r   my-chunk   }
#| eval: false
#|cache:  false
print(42)
```"#;

        let (state, uri) = create_test_state("file:///test.knot", text, false).await;
        let params = create_formatting_params(&uri);
        let result = handle_formatting(&state, params).await.unwrap();

        assert!(result.is_some());
        let edits = result.unwrap();
        let new_text = &edits[0].new_text;

        assert!(new_text.contains("```{r my-chunk}"));
        assert!(new_text.contains("#| eval: false"));
        assert!(new_text.contains("#| cache: false"));
    }
}
