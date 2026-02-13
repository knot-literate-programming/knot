// Diagnostics for Knot documents
//
// Provides error detection and validation:
// - Parsing errors (malformed chunks, unmatched brackets)
// - Invalid chunk options
// - Missing dependencies
// - Invalid inline expressions
// - Runtime errors and warnings from R/Python (via cache)

use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::executors::error_utils::extract_line_from_traceback;
use knot_core::get_cache_dir;
use knot_core::parser::parse_document;
use tower_lsp::lsp_types::*;

/// Helper to convert knot_core Position to LSP Position
fn to_lsp_pos(pos: knot_core::parser::Position) -> Position {
    Position {
        line: pos.line as u32,
        character: pos.column as u32,
    }
}

/// Generate diagnostics for a document
pub fn get_diagnostics(uri: &Url, text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

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
            let line_len = line_text.len() as u32;

            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: target_line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: target_line as u32,
                        character: line_len,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("knot".to_string()),
                message: error.message.clone(),
                ..Diagnostic::default()
            });
        }
    }

    // 2. Runtime Diagnostics (from Cache)
    if let Ok(path) = uri.to_file_path() {
        if let Ok(project_root) = Config::find_project_root(&path) {
            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
            let cache_dir = get_cache_dir(&project_root, file_stem);

            if let Ok(cache) = Cache::new(cache_dir) {
                for chunk_cache in cache.metadata.chunks {
                    // Match cache entry with parsed chunk
                    if let Some(parsed_chunk) = doc.chunks.get(chunk_cache.index) {
                        // Add Warnings
                        for warning in chunk_cache.warnings {
                            diagnostics.push(Diagnostic {
                                range: Range {
                                    start: to_lsp_pos(parsed_chunk.range.start),
                                    end: to_lsp_pos(parsed_chunk.range.end),
                                },
                                severity: Some(DiagnosticSeverity::WARNING),
                                source: Some(format!("knot-{}", chunk_cache.language)),
                                message: warning.message.to_string(),
                                ..Diagnostic::default()
                            });
                        }

                        // Add Fatal Error with precise line if possible
                        if let Some(error) = chunk_cache.error {
                            let msg = error
                                .message
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "Execution error".to_string());

                            // Try to find exact line within chunk
                            let error_line_in_chunk = extract_line_from_traceback(&error.traceback);

                            let range = if let Some(line_num) = error_line_in_chunk {
                                // line_num is 1-indexed relative to the code start
                                let absolute_line = parsed_chunk.code_range.start.line + line_num - 1;
                                let line_text = text.lines().nth(absolute_line).unwrap_or("");
                                Range {
                                    start: Position {
                                        line: absolute_line as u32,
                                        character: 0,
                                    },
                                    end: Position {
                                        line: absolute_line as u32,
                                        character: line_text.len() as u32,
                                    },
                                }
                            } else {
                                // Fallback: highlight entire chunk
                                Range {
                                    start: to_lsp_pos(parsed_chunk.range.start),
                                    end: to_lsp_pos(parsed_chunk.range.end),
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
    }

    // Check for errors in inline expressions
    for inline in doc.inline_exprs {
        for error in inline.errors {
            let (line, col) = byte_offset_to_line_col(text, inline.start);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                    end: Position {
                        line: line as u32,
                        character: (col + 1) as u32,
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

/// Helper to convert byte offset to line/col (UTF-16 aware for LSP)
pub fn byte_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;

    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16();
        }
    }
    (line, col)
}
