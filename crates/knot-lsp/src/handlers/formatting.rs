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

        // Format code with external tools
        let formatted_code =
            knot_core::compiler::formatters::format_code(&chunk.code, &chunk.language).ok();
        
        // Append formatted chunk (structural + code)
        clean_knot_text.push_str(&chunk.format(formatted_code.as_deref()));
        
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
            let _ = proxy.send_notification("textDocument/didOpen", serde_json::json!({
                "textDocument": {
                    "uri": virtual_uri,
                    "languageId": "typst",
                    "version": version + 1000, // Use a high version to avoid conflicts
                    "text": typst_mask
                }
            })).await;

            // Request formatting
            let resp = proxy.send_request("textDocument/formatting", serde_json::json!({
                "textDocument": { "uri": virtual_uri },
                "options": params.options
            })).await;

            match resp {
                Ok(res) => {
                    if let Some(edits_val) = res.get("result") {
                        if let Ok(edits) = serde_json::from_value::<Vec<TextEdit>>(edits_val.clone()) {
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
                start: Position { line: 0, character: 0 },
                end: Position {
                    line: text.lines().count() as u32,
                    character: text.lines().last().unwrap_or("").len() as u32,
                },
            },
            new_text: final_text,
        }]))
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
            let start_idx = line.char_indices().nth(start_char).map(|(i, _)| i).unwrap_or(line.len());
            let end_idx = line.char_indices().nth(end_char).map(|(i, _)| i).unwrap_or(line.len());
            line.replace_range(start_idx..end_idx, &edit.new_text);
        } else {
            let start_idx = lines[start_line].char_indices().nth(start_char).map(|(i, _)| i).unwrap_or(lines[start_line].len());
            let end_idx = lines[end_line].char_indices().nth(end_char).map(|(i, _)| i).unwrap_or(lines[end_line].len());
            
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
    if original_doc.chunks.len() != formatted_doc.chunks.len() || 
       original_doc.inline_exprs.len() != formatted_doc.inline_exprs.len() {
        log::warn!("Formatting mismatch: element count changed. Falling back to clean Knot.");
        return clean_knot.to_string();
    }

    let mut final_text = String::with_capacity(formatted_typst.len());
    let mut last_pos = 0;

    // 3. Build elements list
    let mut elements: Vec<(usize, usize, bool, usize)> = Vec::new();
    for (i, chunk) in formatted_doc.chunks.iter().enumerate() {
        elements.push((chunk.start_byte, chunk.end_byte, true, i));
    }
    for (i, inline) in formatted_doc.inline_exprs.iter().enumerate() {
        elements.push((inline.start, inline.end, false, i));
    }
    elements.sort_by_key(|e| e.0);

    // 4. Substitution
    for (start, end, is_chunk, index) in elements {
        // Append Typst text before the element
        final_text.push_str(&formatted_typst[last_pos..start]);

        if is_chunk {
            // A. Detect indentation provided by Typst (Tinymist)
            let line_start = formatted_typst[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let indentation = &formatted_typst[line_start..start];
            let indent_str = if indentation.chars().all(|c| c.is_whitespace()) {
                indentation
            } else {
                ""
            };

            // B. Prepare the chunk with the NEW indentation from Typst
            let mut clean_chunk = original_doc.chunks[index].clone();
            clean_chunk.base_indentation = indent_str.to_string();
            
            // C. Format and append (format() now handles the indentation)
            final_text.push_str(&clean_chunk.format(None));
        } else {
            let clean_inline = &original_doc.inline_exprs[index];
            final_text.push_str(&format!("`{{{}}} {}`", clean_inline.language, clean_inline.code));
        }
        last_pos = end;
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
