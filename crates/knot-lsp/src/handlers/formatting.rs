fn project_formatter(
    uri: &tower_lsp::lsp_types::Url,
    formatter: knot_core::CodeFormatter,
) -> tower_lsp::jsonrpc::Result<knot_core::CodeFormatter> {
    let Ok(path) = uri.to_file_path() else {
        return Ok(formatter);
    };
    let (config, _) = knot_core::Config::find_and_load(&path).map_err(|error| {
        tower_lsp::jsonrpc::Error::invalid_params(format!(
            "Cannot load tool configuration: {error:#}"
        ))
    })?;
    Ok(formatter.with_project_tools(&config.tools))
}

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
    let formatter = project_formatter(uri, formatter)?;
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
    let (text, version) = {
        let docs = state.documents.read().await;
        let Some(doc) = docs.get(uri) else {
            return Ok(None);
        };
        (doc.text.clone(), doc.version)
    };
    let doc = Document::parse(text.clone());
    if !doc.errors.is_empty() {
        return Err(Error::invalid_params(doc.errors.join("\n")));
    }
    let Some(chunk) = doc.chunks.into_iter().find(|chunk| {
        pos.line as usize >= chunk.range.start.line && pos.line as usize <= chunk.range.end.line
    }) else {
        return Ok(None);
    };
    if !chunk.errors.is_empty() {
        return Err(Error::invalid_params(
            chunk
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    let start = chunk.start_byte;
    // Keep the closing line ending outside the replacement, including CRLF.
    let end = chunk.end_byte - usize::from(text[..chunk.end_byte].ends_with('\r'));
    let formatter = state
        .formatter
        .read()
        .await
        .clone()
        .unwrap_or_else(|| knot_core::CodeFormatter::new(None, None));
    let formatter = project_formatter(uri, formatter)?;
    let formatted = tokio::task::spawn_blocking(move || {
        formatter
            .format_code(&chunk.code, &chunk.language)
            .map(|code| chunk.format(Some(&code), None))
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
    if formatted == text[start..end] {
        return Ok(None);
    }
    let mapper = crate::position_mapper::PositionMapper::new(&text, &text);
    // The client must also reject edits if its buffer changed before didChange
    // reached the server or while workspace/applyEdit was in flight.
    Ok(Some(WorkspaceEdit {
        document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: Some(version),
            },
            edits: vec![OneOf::Left(TextEdit {
                range: Range {
                    start: mapper.position_at_offset(start),
                    end: mapper.position_at_offset(end),
                },
                new_text: formatted,
            })],
        }])),
        ..Default::default()
    }))
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

    #[test]
    fn project_formatter_overrides_editor_paths_for_the_document() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("chapters")).unwrap();
        std::fs::write(
            dir.path().join("knot.toml"),
            "[tools]\nruff = './missing-project-ruff'\n",
        )
        .unwrap();
        let uri = Url::from_file_path(dir.path().join("chapters/new.knot")).unwrap();
        let formatter = project_formatter(&uri, fixture_formatter()).unwrap();
        let error = formatter.format_code("x=1", "python").unwrap_err();
        assert!(
            format!("{error:#}").contains("missing-project-ruff"),
            "{error:#}"
        );
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
    fn fixture_formatter() -> knot_core::CodeFormatter {
        static BIN: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = BIN.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap();
            let output = std::process::Command::new("rustc")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/formatter.rs"
                ))
                .arg("-o")
                .arg(dir.path().join("formatter.exe"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            dir
        });
        knot_core::CodeFormatter::new(None, Some(dir.path().join("formatter.exe")))
    }

    async fn chunk_state(text: &str) -> (ServerState, Url) {
        let state = ServerState::new();
        let uri = Url::parse("file:///chunk.knot").unwrap();
        *state.formatter.write().await = Some(fixture_formatter());
        state.documents.write().await.insert(
            uri.clone(),
            crate::state::DocumentState {
                text: text.into(),
                version: 7,
                mapper: PositionMapper::new(text, text),
                opened_in_tinymist: false,
                knot_diagnostics: vec![],
                tinymist_diagnostics: vec![],
            },
        );
        (state, uri)
    }

    #[tokio::test]
    async fn chunk_formatting_preserves_surroundings_and_versions_the_edit() {
        for newline in ["\n", "\r\n"] {
            let text = "Before 🦀\n  ```{python result}\n  #| eval: false\n  x=1\n  ```\nAfter 🦀"
                .replace('\n', newline);
            let (state, uri) = chunk_state(&text).await;
            let edit = handle_format_chunk(&state, &uri, Position::new(3, 3))
                .await
                .unwrap()
                .unwrap();
            assert!(edit.changes.is_none());
            let Some(DocumentChanges::Edits(edits)) = edit.document_changes else {
                panic!("missing versioned edits")
            };
            assert_eq!(edits[0].text_document.version, Some(7));
            assert_eq!(edits[0].text_document.uri, uri);
            let OneOf::Left(edit) = &edits[0].edits[0] else {
                panic!("unexpected annotated edit")
            };
            let doc = Document::parse(text.clone());
            let chunk = &doc.chunks[0];
            let end = chunk.end_byte - usize::from(text[..chunk.end_byte].ends_with('\r'));
            let mapper = PositionMapper::new(&text, &text);
            assert_eq!(
                edit.range.start,
                mapper.position_at_offset(chunk.start_byte)
            );
            assert_eq!(edit.range.end, mapper.position_at_offset(end));
            let updated = format!(
                "{}{}{}",
                &text[..chunk.start_byte],
                edit.new_text,
                &text[end..]
            );
            assert!(updated.starts_with(&format!("Before 🦀{newline}")));
            assert!(updated.ends_with(&format!("{newline}After 🦀")));
            let parsed = Document::parse(updated.clone());
            assert_eq!(parsed.chunks[0].code.trim(), "x = 1");
            assert_eq!(parsed.chunks[0].label.as_deref(), Some("result"));
            assert_eq!(parsed.chunks[0].base_indentation, "  ");
            state.documents.write().await.get_mut(&uri).unwrap().text = updated;
            assert!(
                handle_format_chunk(&state, &uri, Position::new(3, 0))
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                handle_format_chunk(&state, &uri, Position::new(0, 0))
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[tokio::test]
    async fn chunk_formatter_and_parser_failures_produce_no_edits() {
        for (text, message) in [
            ("```{python}\nFAIL\n```", "fixture syntax error"),
            (
                "```{python}\n#| unknown-option: 1\nx=1\n```",
                "Unknown chunk option",
            ),
            ("```{python}\nx=1", "Unclosed"),
        ] {
            let (state, uri) = chunk_state(text).await;
            let error = handle_format_chunk(&state, &uri, Position::new(1, 0))
                .await
                .unwrap_err();
            assert!(error.message.contains(message), "{error}");
            assert_eq!(state.documents.read().await[&uri].text, text);
        }
        let (state, uri) = chunk_state("```{r}\nx<-1\n```").await;
        let missing = tempfile::tempdir().unwrap();
        *state.formatter.write().await = Some(knot_core::CodeFormatter::new(
            Some(missing.path().join("missing-air")),
            None,
        ));
        assert!(
            handle_format_chunk(&state, &uri, Position::new(1, 0))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn changed_or_closed_document_rejects_a_slow_chunk_formatter() {
        for close in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let gate = dir.path().join("gate");
            let text = format!("```{{python}}\n# WAIT {}\nx=1\n```", gate.display());
            let (state, uri) = chunk_state(&text).await;
            let task = tokio::spawn({
                let state = state.clone();
                let uri = uri.clone();
                async move { handle_format_chunk(&state, &uri, Position::new(2, 0)).await }
            });
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !gate.with_extension("started").exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            {
                let mut docs = state.documents.write().await;
                if close {
                    docs.remove(&uri);
                } else {
                    let doc = docs.get_mut(&uri).unwrap();
                    doc.version += 1;
                    doc.text.push_str("\nNew text");
                }
            }
            std::fs::write(gate.with_extension("release"), "ready").unwrap();
            assert_eq!(
                task.await.unwrap().unwrap_err().code,
                ErrorCode::ContentModified
            );
        }
    }
    #[tokio::test]
    async fn chunk_request_reports_editor_acceptance_rejection_and_rpc_errors() {
        use futures::{SinkExt, StreamExt};
        use tower_lsp::jsonrpc::{Request, Response};
        for outcome in ["accepted", "rejected", "rpc_error", "formatter_error"] {
            let text = if outcome == "formatter_error" {
                "```{python}\nFAIL\n```"
            } else {
                "```{python}\nx=1\n```"
            };
            let (state, uri) = chunk_state(text).await;
            let (mut service, socket) =
                tower_lsp::LspService::new(|client| crate::KnotLanguageServer {
                    client,
                    state,
                    root_uri: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
                });
            tower::Service::call(&mut service, Request::build("initialize").id(1)
                .params(serde_json::json!({"capabilities": {"workspace": {"applyEdit": true, "workspaceEdit": {"documentChanges": true}}}})).finish()).await.unwrap();
            let (mut requests, mut responses) = socket.split();
            let server = service.inner();
            let invoke = server.handle_custom_format_chunk(crate::FormatChunkParams {
                uri,
                position: Position::new(1, 0),
            });
            let client = async {
                // The handler logs the request before asking the editor to apply it.
                let log = requests.next().await.unwrap();
                assert_eq!(log.method(), "window/logMessage");
                if outcome == "formatter_error" {
                    return;
                }
                let request = requests.next().await.unwrap();
                assert_eq!(request.method(), "workspace/applyEdit");
                assert_eq!(
                    request.params().unwrap()["edit"]["documentChanges"][0]["textDocument"]["version"],
                    7
                );
                let result = match outcome {
                    "accepted" => Ok(serde_json::json!({"applied": true})),
                    "rejected" => {
                        Ok(serde_json::json!({"applied": false, "failureReason": "buffer changed"}))
                    }
                    _ => Err(Error::invalid_params("client unavailable")),
                };
                responses
                    .send(Response::from_parts(request.id().unwrap().clone(), result))
                    .await
                    .unwrap();
            };
            let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                tokio::join!(invoke, client)
            })
            .await
            .unwrap();
            match outcome {
                "accepted" => assert_eq!(result.unwrap()["status"], "success"),
                "rejected" => assert!(result.unwrap_err().message.contains("buffer changed")),
                "rpc_error" => assert!(result.unwrap_err().message.contains("client unavailable")),
                _ => assert!(result.unwrap_err().message.contains("fixture syntax error")),
            }
        }
    }
}
