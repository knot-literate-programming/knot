use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;

/// Format the document through the same synchronous engine as knot-cli.
pub async fn handle_formatting(
    state: &ServerState,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;
    let (text, version) = {
        let docs = state.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        (doc.text.clone(), doc.version)
    };
    let formatter = state
        .formatter
        .read()
        .await
        .clone()
        .unwrap_or_else(|| knot_core::CodeFormatter::new(None, None));
    let source = text.clone();
    let formatted = tokio::task::spawn_blocking(move || {
        knot_core::formatting::format_document(&source, &formatter)
    })
    .await
    .map_err(|error| Error::invalid_params(format!("Formatting worker failed: {error}")))?
    .map_err(|error| Error::invalid_params(format!("Formatting failed: {error:#}")))?;
    if !state
        .documents
        .read()
        .await
        .get(uri)
        .is_some_and(|doc| doc.version == version && doc.text == text)
    {
        return Err(Error {
            code: ErrorCode::ContentModified,
            message: "Document changed during formatting".into(),
            data: None,
        });
    }
    if formatted == text {
        return Ok(None);
    }
    Ok(Some(vec![TextEdit {
        range: Range {
            start: Position::new(0, 0),
            end: document_end_position(&text),
        },
        new_text: formatted,
    }]))
}

/// Returns the LSP end-of-document position (UTF-16 column, 0-based line).
///
/// `text.lines().count()` is 1-based and collapses the virtual empty line
/// produced by a trailing `\n`, so it cannot be used directly as an LSP line
/// number.  Instead we iterate characters once to track line/column correctly.
fn document_end_position(text: &str) -> Position {
    let mut line = 0u32;
    let mut col_utf16 = 0u32;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            col_utf16 = 0;
        } else {
            col_utf16 += ch.len_utf16() as u32;
        }
    }
    Position {
        line,
        character: col_utf16,
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

    // 2. Parse document to find the chunk under cursor (always succeeds)
    let doc = Document::parse(text.clone());

    let line = pos.line as usize;
    let target_chunk = doc
        .chunks
        .iter()
        .find(|c| line >= c.range.start.line && line <= c.range.end.line);

    if let Some(chunk) = target_chunk {
        // 3. Format the chunk
        let formatted_code = {
            let fmt = state.formatter.read().await.clone();
            if let Some(f) = fmt {
                let code = chunk.code.clone();
                let lang = chunk.language.clone();
                tokio::task::spawn_blocking(move || f.format_code(&code, &lang))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
            } else {
                None
            }
        };
        let formatted = chunk.format(formatted_code.as_deref(), None);

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
    use crate::position_mapper::PositionMapper;

    async fn format(
        text: &str,
        formatter: knot_core::CodeFormatter,
    ) -> Result<Option<Vec<TextEdit>>> {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.knot").unwrap();
        *state.formatter.write().await = Some(formatter);
        state.documents.write().await.insert(
            uri.clone(),
            crate::state::DocumentState {
                text: text.into(),
                version: 1,
                mapper: PositionMapper::new(text, text),
                opened_in_tinymist: false,
                knot_diagnostics: Vec::new(),
                tinymist_diagnostics: Vec::new(),
            },
        );
        handle_formatting(
            &state,
            DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri },
                // Document style is shared with the CLI, not dependent on editor tabs.
                options: FormattingOptions {
                    tab_size: 8,
                    insert_spaces: false,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            },
        )
        .await
    }

    #[tokio::test]
    async fn editor_uses_shared_style_without_tinymist_and_is_idempotent() {
        let text = "#let x=  2\nValue 🦀 `{r, output=false} x <- 15`.";
        let formatter = knot_core::CodeFormatter::new(None, None);
        let expected = knot_core::formatting::format_document(text, &formatter).unwrap();
        let edits = format(text, formatter.clone()).await.unwrap().unwrap();
        assert_eq!(edits[0].new_text, expected);
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("main.knot");
        std::fs::write(&file, text).unwrap();
        knot_cli::format_file(&file, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), edits[0].new_text);
        assert!(!knot_cli::format_file(&file, true).unwrap());
        assert_eq!(edits[0].range.end, document_end_position(text));
        assert!(format(&expected, formatter).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn errors_never_return_partial_editor_edits() {
        let missing = tempfile::tempdir().unwrap();
        let formatter = knot_core::CodeFormatter::new(
            Some(missing.path().join("air")),
            Some(missing.path().join("ruff")),
        );
        for text in [
            "#let x= 2\n```{r}\nx <- 1\n```",
            "#let x = (",
            "```{python}\nmissing fence",
        ] {
            assert!(format(text, formatter.clone()).await.is_err());
        }
    }

    #[test]
    fn edit_range_preserves_utf16_and_final_newline() {
        assert_eq!(document_end_position("a🦀"), Position::new(0, 3));
        assert_eq!(document_end_position("a🦀\n"), Position::new(1, 0));
        assert_eq!(document_end_position("a\nb"), Position::new(1, 1));
    }
}
