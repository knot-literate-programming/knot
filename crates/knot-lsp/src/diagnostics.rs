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

/// The PDF renders the same severity (red error or yellow warning).
fn lsp_severity(severity: knot_core::parser::ast::Severity) -> DiagnosticSeverity {
    match severity {
        knot_core::parser::ast::Severity::Error => DiagnosticSeverity::ERROR,
        knot_core::parser::ast::Severity::Warning => DiagnosticSeverity::WARNING,
    }
}

/// Diagnostics of the project configuration, when `uri` is the project's main
/// file: errors for missing includes (on the injection line, where the PDF
/// renders them) and `knot.toml` warnings (on the first line; the PDF lists
/// them at the end of the document).
fn project_diagnostics(uri: &Url, text: &str) -> Vec<Diagnostic> {
    let Ok(path) = uri.to_file_path() else {
        return Vec::new();
    };
    let Ok((config, root)) = Config::find_and_load(&path) else {
        return Vec::new();
    };
    let is_main = config
        .document
        .main
        .as_deref()
        .is_some_and(|main| root.join(main).canonicalize().ok() == path.canonicalize().ok());
    if !is_main {
        return Vec::new();
    }
    let lines = text.lines().count().max(1);
    let on_line = |line: usize, severity, message| {
        let width = text.lines().nth(line).unwrap_or("").encode_utf16().count() as u32;
        Diagnostic {
            range: Range {
                start: Position {
                    line: line as u32,
                    character: 0,
                },
                end: Position {
                    line: line as u32,
                    character: width.max(1),
                },
            },
            severity: Some(severity),
            source: Some("knot".to_string()),
            message,
            ..Diagnostic::default()
        }
    };
    let injection = (knot_core::project::find_placeholder_line(text) - 1).min(lines - 1);
    let missing = config
        .document
        .includes
        .iter()
        .flatten()
        .filter(|name| matches!(knot_core::project::resolve_include(&root, name), Ok(None)))
        .map(|name| {
            on_line(
                injection,
                DiagnosticSeverity::ERROR,
                knot_core::project::missing_include_message(name),
            )
        });
    // The compiler checks the same sources (includes as saved on disk).
    let includes: Vec<String> = config
        .document
        .includes
        .iter()
        .flatten()
        .filter_map(|name| {
            knot_core::project::resolve_include(&root, name)
                .ok()
                .flatten()
        })
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    let sources: Vec<&str> = std::iter::once(text)
        .chain(includes.iter().map(String::as_str))
        .collect();
    let codly = knot_core::codly::missing_import_warning(&config, &sources);
    let warnings = config
        .warnings
        .iter()
        .cloned()
        .chain(codly)
        .map(|warning| on_line(0, DiagnosticSeverity::WARNING, warning));
    missing.chain(warnings).collect()
}

/// Route the Typst diagnostics of a project's assembled `main.typ` to the
/// source files they concern (main or include), through the '#KNOT-SYNC'
/// markers of `typ_content`. A diagnostic that maps to no source line (the
/// embedded library, generated code) goes on the first line of the main file,
/// so that no error is lost. Ranges cover the whole source line, read with
/// `source_line`.
pub fn route_project_diagnostics(
    typ_content: &str,
    project_root: &std::path::Path,
    main_file: &std::path::Path,
    diagnostics: Vec<Diagnostic>,
    source_line: impl Fn(&std::path::Path, usize) -> Option<String>,
) -> std::collections::HashMap<std::path::PathBuf, Vec<Diagnostic>> {
    let blocks = knot_core::sync::parse_knot_markers(typ_content);
    let mut routed: std::collections::HashMap<_, Vec<_>> = std::collections::HashMap::new();
    for mut diagnostic in diagnostics {
        let (path, line) = knot_core::sync::map_typ_line_to_knot(
            diagnostic.range.start.line as usize,
            &blocks,
            project_root,
        )
        .unwrap_or_else(|| (main_file.to_path_buf(), 0));
        let width =
            source_line(&path, line).map_or(1, |text| text.encode_utf16().count().max(1) as u32);
        diagnostic.range = Range {
            start: Position {
                line: line as u32,
                character: 0,
            },
            end: Position {
                line: line as u32,
                character: width,
            },
        };
        routed.entry(path).or_default().push(diagnostic);
    }
    routed
}

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

    // Project diagnostics (missing includes, knot.toml warnings) are reported
    // on the main file, as the PDF shows them in the main document.
    diagnostics.extend(project_diagnostics(uri, text));

    // Missing `depends` files, resolved like the compiler does.
    let project = uri
        .to_file_path()
        .ok()
        .and_then(|path| Config::find_and_load(&path).ok());

    // Check for errors in chunks (parsing/options, missing dependencies)
    for chunk in &doc.chunks {
        let missing = project
            .as_ref()
            .map(|(config, root)| knot_core::dependency_errors(chunk, config, root, text))
            .unwrap_or_default();
        for error in chunk.errors.iter().chain(&missing) {
            let target_line = if let Some(offset) = error.line_offset {
                chunk.range.start.line + offset
            } else {
                chunk.range.start.line
            };

            let line_text = text.lines().nth(target_line).unwrap_or("");
            let line_len_utf16 = line_text.encode_utf16().count() as u32;

            let severity = lsp_severity(error.severity);

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
            diagnostics.push(Diagnostic {
                range: Range {
                    start: mapper.position_at_offset(inline.start),
                    end: mapper.position_at_offset(inline.end),
                },
                severity: Some(lsp_severity(error.severity)),
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

    #[test]
    fn option_diagnostics_use_their_own_severity() {
        let uri = Url::parse("file:///main.knot").unwrap();
        let text = "```{r}\n#| unknown-opt: 1\nx\n```\n\n```{r}\n#| eval: maybe\nx\n```\n\n`{r, foo=1} 1`\n";
        let severities: Vec<_> = get_diagnostics(&uri, text, false)
            .into_iter()
            .map(|d| d.severity.unwrap())
            .collect();
        assert_eq!(
            severities,
            [
                DiagnosticSeverity::WARNING,
                DiagnosticSeverity::ERROR,
                DiagnosticSeverity::WARNING
            ]
        );
    }

    #[test]
    fn typst_diagnostics_of_the_assembled_document_reach_their_source_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\nincludes = ['chapter.knot']\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("main.knot"),
            "= Main\n\n/* KNOT-INJECT-CHAPTERS */\n",
        )
        .unwrap();
        let chapter = "= Chapter\n\nSome prose.\n#import \"@preview/pkg:0.7.0\" as gg\n";
        std::fs::write(root.path().join("chapter.knot"), chapter).unwrap();
        let build = knot_core::project::ProjectBuild::prepare_preview(
            root.path(),
            &Default::default(),
            Default::default(),
        )
        .unwrap();
        let typ = build
            .phase0(knot_core::Phase0Mode::Pending)
            .unwrap()
            .typ_content;
        let at = |line: usize| Diagnostic {
            range: Range {
                start: Position {
                    line: line as u32,
                    character: 8,
                },
                end: Position {
                    line: line as u32,
                    character: 30,
                },
            },
            message: "package requires Typst 0.15.0 or newer".into(),
            ..Diagnostic::default()
        };
        let import = typ
            .lines()
            .position(|l| l.starts_with("#import \"@preview/pkg"))
            .unwrap();
        // Line 1 is in the embedded library: it maps to no source line.
        let routed = route_project_diagnostics(
            &typ,
            root.path(),
            &root.path().join("main.knot"),
            vec![at(import), at(1)],
            |path, line| {
                std::fs::read_to_string(path)
                    .ok()?
                    .lines()
                    .nth(line)
                    .map(String::from)
            },
        );
        let in_chapter = &routed[&root.path().join("chapter.knot")];
        assert_eq!(in_chapter.len(), 1);
        assert_eq!(in_chapter[0].range.start.line, 3);
        assert_eq!(in_chapter[0].range.end.character, 34);
        let in_main = &routed[&root.path().join("main.knot")];
        assert_eq!(in_main[0].range.start.line, 0);
        assert_eq!(in_main[0].message, "package requires Typst 0.15.0 or newer");
    }

    #[test]
    fn missing_dependencies_are_errors_on_the_depends_line() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\n",
        )
        .unwrap();
        std::fs::create_dir(root.path().join("data")).unwrap();
        std::fs::write(root.path().join("data/here.csv"), "a\n").unwrap();
        let text = "```{r}\n#| depends: [data/here.csv, data/gone.csv]\nx <- 1\n```\n";
        let main = root.path().join("main.knot");
        std::fs::write(&main, text).unwrap();
        let errors = get_diagnostics(&Url::from_file_path(&main).unwrap(), text, false);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].range.start.line, 1);
        assert_eq!(errors[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            errors[0].message,
            knot_core::missing_dependency_message(std::path::Path::new("data/gone.csv"))
        );
    }

    #[test]
    fn codly_options_without_a_codly_import_are_a_warning_on_the_main_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\n\n[chunk-defaults]\ncodly-lang-outset = '(x: 0pt)'\n",
        )
        .unwrap();
        let main = root.path().join("main.knot");
        let uri = Url::from_file_path(&main).unwrap();
        let text = "= Title\n";
        std::fs::write(&main, text).unwrap();
        let warnings = get_diagnostics(&uri, text, false);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(
            warnings[0].message,
            knot_core::codly::missing_import_message(&["codly-lang-outset".into()])
        );
        let imported = "#import \"@preview/codly:1.3.0\": *\n#show: codly-init\n= Title\n";
        assert!(get_diagnostics(&uri, imported, false).is_empty());
    }

    #[test]
    fn missing_includes_are_reported_on_the_placeholder_of_the_main_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\nincludes = ['here.knot', 'gone.knot']\n",
        )
        .unwrap();
        std::fs::write(root.path().join("here.knot"), "").unwrap();
        let text = "= Title\n\n/* KNOT-INJECT-CHAPTERS */\n";
        let main = root.path().join("main.knot");
        std::fs::write(&main, text).unwrap();
        let errors = get_diagnostics(&Url::from_file_path(&main).unwrap(), text, false);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].range.start.line, 2);
        assert!(errors[0].message.contains("gone.knot"));
        // Other files of the project do not repeat the error.
        let here = root.path().join("here.knot");
        assert!(get_diagnostics(&Url::from_file_path(&here).unwrap(), "", false).is_empty());
    }

    #[test]
    fn non_reusable_snapshot_warning_reaches_the_editor_as_a_warning() {
        use knot_core::executors::{ExecutionOutput, ExecutionResult, RuntimeWarning};
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("knot.toml"), "").unwrap();
        let text = "```{python}\nx = 1\n```\n\n```{python}\nclass P:\n    pass\n```\n";
        let main = root.path().join("main.knot");
        std::fs::write(&main, text).unwrap();
        let message = knot_core::defaults::non_reusable_snapshot_message("python");
        let mut cache = Cache::new(get_cache_dir(root.path(), &main)).unwrap();
        let output = ExecutionOutput {
            result: ExecutionResult::Text(String::new()),
            exports: Vec::new(),
            warnings: vec![RuntimeWarning {
                message: message.clone(),
                call: None,
                line: None,
            }],
        };
        cache
            .save_result(1, None, "python".into(), "hash".into(), &output, vec![])
            .unwrap();
        let diagnostics = get_diagnostics(&Url::from_file_path(&main).unwrap(), text, true);
        let warning = diagnostics
            .iter()
            .find(|d| d.message == message)
            .expect("the same message as in the PDF");
        assert_eq!(warning.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(warning.range.start.line, 7, "on the chunk that caused it");
    }

    #[test]
    fn knot_toml_warnings_are_reported_on_the_main_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\n\n[chunk-defaults]\nfig-widht = 5\n",
        )
        .unwrap();
        let main = root.path().join("main.knot");
        std::fs::write(&main, "= Title\n").unwrap();
        let diagnostics = get_diagnostics(&Url::from_file_path(&main).unwrap(), "= Title\n", false);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostics[0].range.start.line, 0);
        assert!(diagnostics[0].message.contains("Did you mean 'fig-width'?"));
    }
}
