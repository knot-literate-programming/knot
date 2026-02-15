use crate::state::ServerState;
use knot_core::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_formatting(
    state: &ServerState,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    // 1. Get current document state
    let (text, version) = {
        let docs = state.documents.read().await;
        match docs.get(uri) {
            Some(doc) => (doc.text.clone(), doc.version),
            _ => return Ok(None),
        }
    };

    // 2. Parse document
    let doc = match Document::parse(text.clone()) {
        Ok(doc) => doc,
        Err(_) => return Ok(None),
    };

    // --- PHASE A: Internal Chunk Formatting (Air/Ruff) ---
    let mut clean_knot_text = String::with_capacity(text.len());
    let mut last_pos = 0;

    for chunk in &doc.chunks {
        // Append text before chunk
        if chunk.start_byte > last_pos {
            clean_knot_text.push_str(&text[last_pos..chunk.start_byte]);
        }

        // Format code with external tools (Air for R, Ruff for Python).
        // CodeFormatter is sync; wrap in spawn_blocking to avoid blocking the runtime.
        let formatted_code = {
            let fmt = state.formatter.read().await;
            if let Some(f) = fmt.as_ref() {
                let f = f.clone();
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

        // Append formatted chunk (structural + code)
        clean_knot_text.push_str(&chunk.format(formatted_code.as_deref(), None));

        last_pos = chunk.end_byte;
    }

    if last_pos < text.len() {
        clean_knot_text.push_str(&text[last_pos..]);
    }

    // --- PHASE B: Global Typst Formatting (Tinymist) ---
    // Generate the structured mask for Tinymist
    let typst_mask = crate::transform::transform_to_typst(&clean_knot_text);
    let virtual_uri = crate::transform::to_virtual_uri(uri);

    let formatted_typst = {
        let mut tinymist_guard = state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            // First, update Tinymist with the current mask
            let _ = proxy
                .send_notification(
                    "textDocument/didOpen",
                    serde_json::json!({
                        "textDocument": {
                            "uri": virtual_uri,
                            "languageId": "typst",
                            "version": version + 1000, // Use a high version to avoid conflicts
                            "text": typst_mask
                        }
                    }),
                )
                .await;

            // Request formatting
            let resp = proxy
                .send_request(
                    "textDocument/formatting",
                    serde_json::json!({
                        "textDocument": { "uri": virtual_uri },
                        "options": params.options
                    }),
                )
                .await;

            match resp {
                Ok(res) => {
                    if let Some(edits_val) = res.get("result") {
                        if let Ok(edits) =
                            serde_json::from_value::<Vec<TextEdit>>(edits_val.clone())
                        {
                            // Apply edits to the mask to get the final Typst structure
                            apply_edits(&typst_mask, edits)
                        } else {
                            typst_mask
                        }
                    } else {
                        typst_mask
                    }
                }
                Err(_) => typst_mask,
            }
        } else {
            typst_mask
        }
    };

    // --- PHASE C: Final Document Reconstruction ---
    // We need to extract the clean Knot chunks from the masked document
    // and re-insert them into the formatted Typst structure.
    let final_text = reconstruct_knot_document(&formatted_typst, &clean_knot_text);

    if final_text == text {
        Ok(None)
    } else {
        // Return a single full-document replacement for simplicity and robustness
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: document_end_position(&text),
            },
            new_text: final_text,
        }]))
    }
}

/// Converts a UTF-16 column offset to a byte offset within a line.
/// LSP positions use UTF-16 code units; Rust strings are UTF-8.
/// For BMP characters (U+0000–U+FFFF) the two are identical, but characters
/// outside the BMP (emoji, some CJK…) occupy 2 UTF-16 code units and 4 UTF-8
/// bytes, so we must iterate code-point by code-point.
fn utf16_to_byte_offset(line: &str, utf16_col: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_idx, ch) in line.char_indices() {
        if utf16_count >= utf16_col {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
    }
    line.len()
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

/// Robustly apply LSP TextEdits to a string
fn apply_edits(text: &str, mut edits: Vec<TextEdit>) -> String {
    // Sort edits in reverse order by position to keep offsets valid
    edits.sort_by(|a, b| {
        if a.range.start.line != b.range.start.line {
            b.range.start.line.cmp(&a.range.start.line)
        } else {
            b.range.start.character.cmp(&a.range.start.character)
        }
    });

    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    // Add an empty line if the text ends with a newline
    if text.ends_with('\n') {
        lines.push(String::new());
    }

    for edit in edits {
        let start_line = edit.range.start.line as usize;
        let start_char = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_char = edit.range.end.character as usize;

        if start_line >= lines.len() || end_line >= lines.len() {
            continue;
        }

        if start_line == end_line {
            let line = &mut lines[start_line];
            let start_idx = utf16_to_byte_offset(line, start_char);
            let end_idx = utf16_to_byte_offset(line, end_char);
            line.replace_range(start_idx..end_idx, &edit.new_text);
        } else {
            let start_idx = utf16_to_byte_offset(&lines[start_line], start_char);
            let end_idx = utf16_to_byte_offset(&lines[end_line], end_char);

            let mut new_content = lines[start_line][..start_idx].to_string();
            new_content.push_str(&edit.new_text);
            new_content.push_str(&lines[end_line][end_idx..]);

            let new_lines: Vec<String> = new_content.lines().map(|s| s.to_string()).collect();
            lines.splice(start_line..=end_line, new_lines);
        }
    }

    let mut result = lines.join("\n");
    if text.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// A positioned element in the formatted Typst document.
struct Element {
    start: usize,
    end: usize,
    kind: ElementKind,
}

enum ElementKind {
    /// A code chunk; the `usize` is the index into the original document's chunk list.
    Chunk(usize),
    /// An inline expression; the `usize` is the index into the original document's inline list.
    Inline(usize),
}

/// Reconstructs the Knot document by finding Knot elements in the formatted Typst.
fn reconstruct_knot_document(formatted_typst: &str, clean_knot: &str) -> String {
    // 1. Parse both documents
    let original_doc = match knot_core::Document::parse(clean_knot.to_string()) {
        Ok(d) => d,
        Err(_) => return clean_knot.to_string(),
    };
    let formatted_doc = match knot_core::Document::parse(formatted_typst.to_string()) {
        Ok(d) => d,
        Err(_) => return clean_knot.to_string(),
    };

    // 2. Element count check
    if original_doc.chunks.len() != formatted_doc.chunks.len()
        || original_doc.inline_exprs.len() != formatted_doc.inline_exprs.len()
    {
        log::warn!("Formatting mismatch: element count changed. Falling back to clean Knot.");
        return clean_knot.to_string();
    }

    // 2b. Language correspondence (pairwise).
    // The mirror mask preserves fence tags verbatim, so a mismatch means
    // Tinymist altered a raw-block header in an unexpected way.  The index-based
    // substitution below would silently assign the wrong code to the wrong chunk
    // without this guard.
    if !original_doc
        .chunks
        .iter()
        .zip(formatted_doc.chunks.iter())
        .all(|(o, f)| o.language == f.language)
    {
        log::warn!("Formatting mismatch: chunk language changed. Falling back to clean Knot.");
        return clean_knot.to_string();
    }

    let mut final_text = String::with_capacity(formatted_typst.len());
    let mut last_pos = 0;

    // 3. Build elements list
    let mut elements: Vec<Element> = Vec::new();
    for (i, chunk) in formatted_doc.chunks.iter().enumerate() {
        elements.push(Element {
            start: chunk.start_byte,
            end: chunk.end_byte,
            kind: ElementKind::Chunk(i),
        });
    }
    for (i, inline) in formatted_doc.inline_exprs.iter().enumerate() {
        elements.push(Element {
            start: inline.start,
            end: inline.end,
            kind: ElementKind::Inline(i),
        });
    }
    elements.sort_by_key(|e| e.start);

    // 2c. Overlap guard.
    // After sorting by start, each element must begin at or after the previous
    // one ends.  The well-formed parser should never produce overlapping ranges,
    // but if Tinymist returns unexpected content the substitution loop would
    // attempt a backwards string slice and panic.  Catch it here and fall back
    // gracefully instead.
    {
        let mut prev_end = 0usize;
        for elem in &elements {
            if elem.start < prev_end || elem.end < elem.start {
                log::warn!("Formatting mismatch: overlapping element positions. Falling back to clean Knot.");
                return clean_knot.to_string();
            }
            prev_end = elem.end;
        }
    }

    // 4. Substitution
    for elem in elements {
        // Append Typst text before the element
        final_text.push_str(&formatted_typst[last_pos..elem.start]);

        match elem.kind {
            ElementKind::Chunk(i) => {
                // A. Detect indentation provided by Typst (Tinymist)
                let line_start = formatted_typst[..elem.start]
                    .rfind('\n')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let indentation = &formatted_typst[line_start..elem.start];
                let indent_str = if indentation.chars().all(|c| c.is_whitespace()) {
                    indentation
                } else {
                    ""
                };

                // B. Format with the indentation detected by Typst, without
                // cloning or mutating the AST node.
                final_text.push_str(&original_doc.chunks[i].format(None, Some(indent_str)));
            }
            ElementKind::Inline(i) => {
                let clean_inline = &original_doc.inline_exprs[i];
                final_text.push_str(&format!(
                    "`{{{}}} {}`",
                    clean_inline.language, clean_inline.code
                ));
            }
        }
        last_pos = elem.end;
    }

    if last_pos < formatted_typst.len() {
        final_text.push_str(&formatted_typst[last_pos..]);
    }

    final_text
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
        let formatted_code = {
            let fmt = state.formatter.read().await;
            if let Some(f) = fmt.as_ref() {
                let f = f.clone();
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
    use crate::state::ServerState;
    use knot_core::CodeFormatter;

    async fn create_test_state(uri: &str, text: &str, with_formatter: bool) -> (ServerState, Url) {
        let state = ServerState::new();
        if with_formatter {
            *state.formatter.write().await = Some(CodeFormatter::new(None, None));
        }
        let url = Url::parse(uri).unwrap();

        let mapper = PositionMapper::new(text, text);
        {
            let mut docs = state.documents.write().await;
            docs.insert(
                url.clone(),
                crate::state::DocumentState {
                    text: text.to_string(),
                    version: 1,
                    mapper,
                    opened_in_tinymist: false,
                    knot_diagnostics: Vec::new(),
                    tinymist_diagnostics: Vec::new(),
                },
            );
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

    // --- Unit tests for reconstruct_knot_document fallbacks ---

    #[test]
    fn test_reconstruct_fallback_count_mismatch() {
        // formatted_typst has two chunks, clean_knot only one → count mismatch
        let clean_knot = "```{r}\nx <- 1\n```\n";
        let formatted_typst = "```{r}\n\n```\n\n```{r}\n\n```\n";
        let result = reconstruct_knot_document(formatted_typst, clean_knot);
        assert_eq!(result, clean_knot);
    }

    #[test]
    fn test_reconstruct_fallback_language_mismatch() {
        // Simulates Tinymist altering the fence language tag
        let clean_knot = "```{r}\nx <- 1\n```\n";
        let formatted_typst = "```{python}\n\n```\n";
        let result = reconstruct_knot_document(formatted_typst, clean_knot);
        assert_eq!(result, clean_knot);
    }

    // --- Unit tests for helpers ---

    #[test]
    fn test_document_end_position_no_trailing_newline() {
        // "a\nb" → line 1, character 1
        let pos = document_end_position("a\nb");
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1);
    }

    #[test]
    fn test_document_end_position_with_trailing_newline() {
        // "a\nb\n" → line 2, character 0  (virtual empty line after final \n)
        let pos = document_end_position("a\nb\n");
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn test_document_end_position_emoji() {
        // 🦀 is U+1F980, outside BMP → 2 UTF-16 units
        let pos = document_end_position("🦀");
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);
    }

    #[test]
    fn test_utf16_to_byte_offset_ascii() {
        let line = "hello";
        assert_eq!(utf16_to_byte_offset(line, 0), 0);
        assert_eq!(utf16_to_byte_offset(line, 3), 3);
        assert_eq!(utf16_to_byte_offset(line, 5), 5); // past-end clamps to len
    }

    #[test]
    fn test_utf16_to_byte_offset_emoji() {
        // "a🦀b": 'a'=1 UTF-16 unit, '🦀'=2 UTF-16 units, 'b'=1 UTF-16 unit
        // UTF-16 offsets: a→0, 🦀→1, b→3
        // Byte offsets:   a→0, 🦀→1, b→5 (4 UTF-8 bytes for 🦀)
        let line = "a🦀b";
        assert_eq!(utf16_to_byte_offset(line, 0), 0); // 'a'
        assert_eq!(utf16_to_byte_offset(line, 1), 1); // '🦀'
        assert_eq!(utf16_to_byte_offset(line, 3), 5); // 'b' (after 4-byte emoji)
        assert_eq!(utf16_to_byte_offset(line, 4), 6); // past-end
    }

    #[test]
    fn test_apply_edits_utf16_emoji() {
        // Replace the emoji in "a🦀b" with "X"
        // LSP sees "a🦀b" as: a=col 0, 🦀=col 1..3, b=col 3
        let text = "a🦀b";
        let edits = vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            },
            new_text: "X".to_string(),
        }];
        assert_eq!(apply_edits(text, edits), "aXb");
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
