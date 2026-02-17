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
use knot_core::executors::error_utils::extract_line_from_traceback;
use knot_core::get_cache_dir;
use knot_core::parser::parse_document;
use tower_lsp::lsp_types::*;

/// Generate diagnostics for a document
pub fn get_diagnostics(uri: &Url, text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Create a mapper for reliable position conversions
    let mapper = PositionMapper::new(text, "");

    // 1. Parsing and Structure Diagnostics
    let doc = parse_document(text);

    // Check for global document errors
    for error in doc.errors {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("knot".to_string()),
            message: error,
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
    }

    // 2. Runtime Diagnostics (from Cache)
    if let Ok(path) = uri.to_file_path()
        && let Ok(project_root) = Config::find_project_root(&path)
    {
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);

        if let Ok(cache) = Cache::new(cache_dir) {
            for chunk_cache in cache.metadata.chunks {
                // Match cache entry with parsed chunk by ordinal index
                // This is stable even when the document is edited before the chunk.
                if let Some(parsed_chunk) = doc
                    .chunks
                    .iter()
                    .find(|c| c.index == chunk_cache.index)
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
