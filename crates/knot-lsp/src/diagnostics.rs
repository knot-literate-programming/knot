// Diagnostics for Knot documents
//
// Provides error detection and validation:
// - Parsing errors (malformed chunks, unmatched brackets)
// - Invalid chunk options
// - Missing dependencies
// - Invalid inline expressions
// - Runtime errors and warnings from R/Python (via cache)

use crate::position_mapper::PositionMapper;
use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::defaults::unsupported_language_message;
use knot_core::executors::error_utils::extract_line_from_traceback;
use knot_core::get_cache_dir;
use knot_core::parser::parse_document;
use tower_lsp::lsp_types::*;

/// Generate diagnostics for a document
pub fn get_diagnostics(uri: &Url, text: &str, include_runtime: bool) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Create a mapper for reliable position conversions
    let mapper = PositionMapper::new(text, "");

    // 1. Parsing and Structure Diagnostics
    let doc = parse_document(text);

    // Document errors (YAML header, unclosed chunk), on the line they concern;
    // the PDF renders the same messages.
    for error in &doc.errors {
        let line_text = text.lines().nth(error.line).unwrap_or("");
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position {
                    line: error.line as u32,
                    character: 0,
                },
                end: Position {
                    line: error.line as u32,
                    character: line_text.encode_utf16().count().max(1) as u32,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("knot".to_string()),
            message: error.message.clone(),
            ..Diagnostic::default()
        });
    }

    // Check for errors in chunks (parsing/options)
    for chunk in &doc.chunks {
        for error in &chunk.errors {
            let target_line = if let Some(offset) = error.line_offset {
                chunk.range.start.line + offset
            } else {
                chunk.range.start.line
            };

            let line_text = text.lines().nth(target_line).unwrap_or("");
            let line_len_utf16 = line_text.encode_utf16().count() as u32;

            let severity = if error.message.contains("Unknown chunk option") {
                DiagnosticSeverity::WARNING
            } else {
                DiagnosticSeverity::ERROR
            };

            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: target_line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: target_line as u32,
                        character: line_len_utf16,
                    },
                },
                severity: Some(severity),
                source: Some("knot".to_string()),
                message: error.message.clone(),
                ..Diagnostic::default()
            });
        }

        if chunk.options.eval != Some(false)
            && let Some(message) = unsupported_language_message(&chunk.language)
        {
            let header = mapper.position_at_offset(chunk.start_byte);
            let header_len = text.lines().nth(header.line as usize).unwrap_or("");
            diagnostics.push(Diagnostic {
                range: Range {
                    start: header,
                    end: Position {
                        line: header.line,
                        character: header_len.encode_utf16().count() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("knot".to_string()),
                message,
                ..Diagnostic::default()
            });
        }
    }

    // Unknown languages in inline expressions
    for inline in &doc.inline_exprs {
        if inline.options.eval != Some(false)
            && let Some(message) = unsupported_language_message(&inline.language)
        {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: mapper.position_at_offset(inline.start),
                    end: mapper.position_at_offset(inline.end),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("knot".to_string()),
                message,
                ..Diagnostic::default()
            });
        }
    }

    // 2. Runtime Diagnostics (from Cache)
    if include_runtime
        && let Ok(path) = uri.to_file_path()
        && let Ok(project_root) = Config::find_project_root(&path)
    {
        let cache_dir = get_cache_dir(&project_root, &path);

        if let Ok(cache) = Cache::new(cache_dir) {
            for chunk_cache in cache.metadata.chunks {
                // Match cache entry with parsed chunk by ordinal index
                // This is stable even when the document is edited before the chunk.
                if let Some(parsed_chunk) = doc.chunks.iter().find(|c| c.index == chunk_cache.index)
                {
                    // Add Warnings
                    for warning in chunk_cache.warnings {
                        let range = if let Some(line_num) = warning.line {
                            // line_num is 1-indexed relative to the code start
                            let absolute_line = parsed_chunk.code_range.start.line + line_num - 1;
                            let line_text = text.lines().nth(absolute_line).unwrap_or("");
                            let line_len_utf16 = line_text.encode_utf16().count() as u32;
                            Range {
                                start: Position {
                                    line: absolute_line as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: absolute_line as u32,
                                    character: line_len_utf16,
                                },
                            }
                        } else {
                            // Fallback: highlight only the closing triple backticks of the chunk
                            let end_pos = mapper.position_at_offset(parsed_chunk.end_byte);
                            Range {
                                start: Position {
                                    line: end_pos.line,
                                    character: end_pos.character.saturating_sub(3),
                                },
                                end: end_pos,
                            }
                        };

                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            source: Some(format!("knot-{}", chunk_cache.language)),
                            message: warning.detailed_message(),
                            ..Diagnostic::default()
                        });
                    }

                    // Add Fatal Error with precise line if possible
                    if let Some(error) = chunk_cache.error {
                        let msg = error.detailed_message();

                        // Try to find exact line within chunk
                        let error_line_in_chunk = extract_line_from_traceback(&error.traceback);

                        let range = if let Some(line_num) = error_line_in_chunk {
                            let absolute_line = parsed_chunk.code_range.start.line + line_num - 1;
                            let line_text = text.lines().nth(absolute_line).unwrap_or("");
                            let line_len_utf16 = line_text.encode_utf16().count() as u32;
                            Range {
                                start: Position {
                                    line: absolute_line as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: absolute_line as u32,
                                    character: line_len_utf16,
                                },
                            }
                        } else {
                            // Fallback: highlight only the closing triple backticks of the chunk
                            let end_pos = mapper.position_at_offset(parsed_chunk.end_byte);
                            Range {
                                start: Position {
                                    line: end_pos.line,
                                    character: end_pos.character.saturating_sub(3),
                                },
                                end: end_pos,
                            }
                        };

                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some(format!("knot-{}", chunk_cache.language)),
                            message: msg,
                            ..Diagnostic::default()
                        });
                    }
                }
            }
        }
    }

    // Check for errors in inline expressions
    for inline in doc.inline_exprs {
        for error in inline.errors {
            let start_pos = mapper.position_at_offset(inline.start);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: start_pos,
                    end: Position {
                        line: start_pos.line,
                        character: start_pos.character + 1, // Highlight `
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("knot".to_string()),
                message: error.message,
                ..Diagnostic::default()
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_languages_are_errors_unless_not_evaluated() {
        let uri = Url::parse("file:///main.knot").unwrap();
        let text = "```{julia}\nx = 1\n```\n\n```{bash}\n#| eval: false\nls\n```\n\n```{py}\nx = 1\n```\n\nValue `{julia} x`.\n";
        let errors: Vec<_> = get_diagnostics(&uri, text, false)
            .into_iter()
            .filter(|d| d.message.starts_with("Unsupported language"))
            .collect();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        );
        assert_eq!(
            errors[0].range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(errors[0].range.end.character, "```{julia}".len() as u32);
        assert_eq!(errors[1].range.start.line, 13);
        assert!(errors[0].message.contains("r and python"));
    }

    #[test]
    fn document_errors_are_reported_on_their_line() {
        let uri = Url::parse("file:///main.knot").unwrap();
        let text = "---\nsnapshots:\n  python: nope\n---\nIntro\n\n```{r}\nx <- 1\n";
        let errors: Vec<_> = get_diagnostics(&uri, text, false)
            .into_iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect();
        let lines: Vec<_> = errors.iter().map(|d| d.range.start.line).collect();
        assert_eq!(lines, [2, 6], "{errors:?}");
        assert!(errors[0].message.starts_with("Invalid YAML header"));
        assert!(errors[1].message.starts_with("Unclosed chunk"));
    }
}
