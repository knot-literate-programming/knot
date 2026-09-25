//! Output formatting backend trait and Typst implementation.

use crate::compiler::ChunkExecutionState;
use crate::executors::{ExecutionOutput, ExecutionResult};
use crate::parser::{Chunk, Layout, ResolvedChunkOptions, Show};
use crate::path_utils::{escape_typst_path, escape_typst_string};
use crate::typst_syntax::{raw_block, string_literal, text_content};
use std::collections::HashMap;

/// Helper function to format a HashMap of options into a Typst function call.
fn format_typst_call(fn_name: &str, options: &HashMap<String, String>) -> String {
    // Sort by key for deterministic output (HashMap iteration order is not stable).
    let mut args: Vec<String> = options
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value))
        .collect();
    args.sort();
    format!("#{}({})", fn_name, args.join(", "))
}

/// Formats a HashMap of codly options into a Typst #codly() function call.
pub fn format_codly_call(options: &HashMap<String, String>) -> String {
    format_typst_call("codly", options)
}

/// Formats a HashMap of codly options into a Typst #local() function call.
pub fn format_local_call(options: &HashMap<String, String>) -> String {
    format_typst_call("local", options)
}

/// Output backend for the compilation pipeline.
///
/// The current implementation targets Typst ([`TypstBackend`]), but the trait is
/// designed to support additional backends in the future — for example LaTeX or
/// Markdown. To add a new backend, implement this trait and wire it up in
/// `compiler/mod.rs` where `TypstBackend` is instantiated.
pub trait Backend {
    /// Formats a processed chunk into the target document syntax.
    ///
    /// `codly_options` is the *merged* set of codly presentation options for this
    /// chunk (global config merged with per-chunk overrides).  It is passed
    /// separately so that the caller never needs to clone the `Chunk` struct.
    fn format_chunk(
        &self,
        chunk: &Chunk,
        codly_options: &HashMap<String, String>,
        resolved_options: &ResolvedChunkOptions,
        output: &ExecutionOutput,
        state: &ChunkExecutionState,
    ) -> String;
}

/// Typst output backend — the only current implementation of [`Backend`].
pub struct TypstBackend;

impl Default for TypstBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TypstBackend {
    /// Creates a new `TypstBackend`.
    pub fn new() -> Self {
        Self
    }
}

impl Backend for TypstBackend {
    fn format_chunk(
        &self,
        chunk: &Chunk,
        codly_options: &HashMap<String, String>,
        resolved_options: &ResolvedChunkOptions,
        output: &ExecutionOutput,
        state: &ChunkExecutionState,
    ) -> String {
        let mut exports = String::new();
        for export in &output.exports {
            let name = format!("\"{}\"", escape_typst_string(&export.name));
            let path = escape_typst_path(&export.path);
            exports.push_str(&format!(
                "#assert(not ({name} in knot-data), message: \"Duplicate Knot data export: \" + {name})\n#let knot-data = knot-data + (({name}): json(\"{path}\"),)\n"
            ));
        }
        // `show: none` hides code and output, never diagnostics about the chunk.
        if matches!(resolved_options.show, Show::None) && chunk.errors.is_empty() {
            return exports;
        }
        let call = chunk_call(chunk, codly_options, resolved_options, output, state, None);
        format!("{exports}{call}")
    }
}

impl TypstBackend {
    /// A chunk whose execution failed: rendered exactly like the chunk itself
    /// (label and caption, code according to `show`, option diagnostics,
    /// presentation options), with `error` — Typst content — added to its
    /// errors. It is rendered even with `show: none`, and references to its
    /// label keep resolving.
    pub(crate) fn format_failed_chunk(
        &self,
        chunk: &Chunk,
        codly_options: &HashMap<String, String>,
        resolved_options: &ResolvedChunkOptions,
        error: &str,
    ) -> String {
        let empty = ExecutionOutput {
            result: ExecutionResult::Text(String::new()),
            exports: Vec::new(),
            warnings: vec![],
        };
        chunk_call(
            chunk,
            codly_options,
            resolved_options,
            &empty,
            &ChunkExecutionState::Ready,
            Some(error),
        )
    }
}

/// The `#code-chunk(...)` (or `#knot-replace(...)`) call of a chunk; `error`
/// is extra error content appended after the chunk's option errors.
fn chunk_call(
    chunk: &Chunk,
    codly_options: &HashMap<String, String>,
    resolved_options: &ResolvedChunkOptions,
    output: &ExecutionOutput,
    state: &ChunkExecutionState,
    error: Option<&str>,
) -> String {
    let mut args = vec![];
    push_base_args(chunk, state, error, &mut args);
    push_warnings_arg(chunk, output, resolved_options, &mut args);
    push_code_arg(chunk, codly_options, resolved_options, &mut args);
    push_output_arg(output, resolved_options, &mut args);
    push_presentation_args(resolved_options, &mut args);

    let fn_name = if matches!(resolved_options.show, Show::Replace) {
        "knot-replace"
    } else {
        "code-chunk"
    };
    format!("#{}({})", fn_name, args.join(", "))
}

// ---------------------------------------------------------------------------
// Private helpers for format_chunk()
// ---------------------------------------------------------------------------

/// Pushes lang, label/caption, is-inert, and parse errors into `args`.
fn push_base_args(
    chunk: &Chunk,
    state: &ChunkExecutionState,
    extra_error: Option<&str>,
    args: &mut Vec<String>,
) {
    args.push(format!("lang: {}", string_literal(&chunk.language)));

    if let Some(label) = &chunk.label
        && !label.trim().is_empty()
    {
        args.push(format!("label: {}", string_literal(label)));
    }
    if let Some(caption) = &chunk.options.caption {
        args.push(format!("caption: [{}]", caption));
    }

    match state {
        ChunkExecutionState::Inert => args.push("is-inert: true".to_string()),
        ChunkExecutionState::Pending => args.push("is-pending: true".to_string()),
        ChunkExecutionState::Modified => args.push("is-modified: true".to_string()),
        ChunkExecutionState::ModifiedCascade => {
            args.push("is-modified-cascade: true".to_string());
        }
        ChunkExecutionState::Ready => {}
    }

    let errors: Vec<_> = chunk
        .errors
        .iter()
        .filter(|e| e.is_error())
        .map(|e| text_content(&e.message))
        .chain(extra_error.map(String::from))
        .collect();
    if !errors.is_empty() {
        args.push(format!("errors: {}", typst_array(&errors)));
    }
}

/// A Typst array literal; `(item,)` is the single-element form.
fn typst_array(items: &[String]) -> String {
    if items.len() == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

/// Pushes the `warnings:` argument based on visibility setting.
fn push_warnings_arg(
    chunk: &Chunk,
    output: &ExecutionOutput,
    resolved_options: &ResolvedChunkOptions,
    args: &mut Vec<String>,
) {
    // Ignored options are always reported; runtime warnings follow the
    // `warnings-visibility` option and are hidden, like the output, by `show: none`.
    let mut warnings: Vec<_> = chunk
        .errors
        .iter()
        .filter(|e| !e.is_error())
        .map(|e| text_content(&e.message))
        .collect();
    let visibility = resolved_options.warnings_visibility;
    if !matches!(visibility, crate::parser::WarningsVisibility::None)
        && !matches!(resolved_options.show, Show::None)
    {
        warnings.extend(output.warnings.iter().map(|w| text_content(&w.message)));
    }
    if warnings.is_empty() {
        return;
    }
    args.push(format!("warnings: {}", typst_array(&warnings)));
    if matches!(visibility, crate::parser::WarningsVisibility::Inline) {
        args.push("warnings-position: \"inline\"".to_string());
    }
}

/// Pushes the `code:` argument (with optional #local() wrapper for per-chunk codly options).
fn push_code_arg(
    chunk: &Chunk,
    codly_options: &HashMap<String, String>,
    resolved_options: &ResolvedChunkOptions,
    args: &mut Vec<String>,
) {
    if matches!(
        resolved_options.show,
        Show::Both | Show::Code | Show::Replace
    ) {
        let code_str = if !codly_options.is_empty() {
            let local_call = format_local_call(codly_options);
            format!(
                "[{}[{}]]",
                local_call,
                raw_block(&chunk.language, chunk.code.trim())
            )
        } else {
            format!("[{}]", raw_block(&chunk.language, chunk.code.trim()))
        };
        args.push(format!("code: {}", code_str));
    } else {
        args.push("code: none".to_string());
    }
}

/// Pushes the `output:` argument from the execution result.
fn push_output_arg(
    output: &ExecutionOutput,
    resolved_options: &ResolvedChunkOptions,
    args: &mut Vec<String>,
) {
    if !matches!(
        resolved_options.show,
        Show::Both | Show::Output | Show::Replace
    ) {
        args.push("output: none".to_string());
        return;
    }

    let output_str = match &output.result {
        ExecutionResult::Text(text) if !text.trim().is_empty() => {
            format!("[{}]", raw_block("output", text.trim()))
        }
        ExecutionResult::Plot(path) => {
            let abs_plot = path.canonicalize().unwrap_or_else(|_| path.clone());
            format!("[#image(\"{}\")]", escape_typst_path(&abs_plot))
        }
        ExecutionResult::DataFrame(csv_path) => {
            let abs_csv = csv_path.canonicalize().unwrap_or_else(|_| csv_path.clone());
            format!(
                "[#{{ let data = csv(\"{}\"); table(columns: data.first().len(), ..data.flatten()) }}]",
                escape_typst_path(&abs_csv)
            )
        }
        ExecutionResult::TextAndPlot { text, plot } => {
            let abs_plot = plot.canonicalize().unwrap_or_else(|_| plot.clone());
            format!(
                "[#image(\"{}\")\n{}]",
                escape_typst_path(&abs_plot),
                raw_block("output", text.trim())
            )
        }
        ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
            let abs_csv = dataframe
                .canonicalize()
                .unwrap_or_else(|_| dataframe.clone());
            let abs_plot = plot.canonicalize().unwrap_or_else(|_| plot.clone());
            format!(
                "[#{{ let data = csv(\"{}\"); table(columns: data.first().len(), ..data.flatten()) }}\n#image(\"{}\")]",
                escape_typst_path(&abs_csv),
                escape_typst_path(&abs_plot)
            )
        }
        _ => "none".to_string(),
    };
    args.push(format!("output: {}", output_str));
}

/// Pushes layout and all styling (code/output/warning box) arguments.
fn push_presentation_args(resolved_options: &ResolvedChunkOptions, args: &mut Vec<String>) {
    // Only add layout when showing both code and output
    if matches!(resolved_options.show, Show::Both) {
        let layout_str = match resolved_options.layout {
            Layout::Horizontal => "horizontal",
            Layout::Vertical => "vertical",
        };
        args.push(format!("layout: \"{}\"", layout_str));
    }

    if let Some(gutter) = &resolved_options.gutter {
        args.push(format!("gutter: {}", gutter));
    }

    if let Some(v) = &resolved_options.code_background {
        args.push(format!("code-background: {}", v));
    }
    if let Some(v) = &resolved_options.code_stroke {
        args.push(format!("code-stroke: {}", v));
    }
    if let Some(v) = &resolved_options.code_radius {
        args.push(format!("code-radius: {}", v));
    }
    if let Some(v) = &resolved_options.code_inset {
        args.push(format!("code-inset: {}", v));
    }

    if let Some(v) = &resolved_options.output_background {
        args.push(format!("output-background: {}", v));
    }
    if let Some(v) = &resolved_options.output_stroke {
        args.push(format!("output-stroke: {}", v));
    }
    if let Some(v) = &resolved_options.output_radius {
        args.push(format!("output-radius: {}", v));
    }
    if let Some(v) = &resolved_options.output_inset {
        args.push(format!("output-inset: {}", v));
    }

    if let Some(v) = &resolved_options.warning_background {
        args.push(format!("warning-background: {}", v));
    }
    if let Some(v) = &resolved_options.warning_stroke {
        args.push(format!("warning-stroke: {}", v));
    }
    if let Some(v) = &resolved_options.warning_radius {
        args.push(format!("warning-radius: {}", v));
    }
    if let Some(v) = &resolved_options.warning_inset {
        args.push(format!("warning-inset: {}", v));
    }

    if let Some(v) = &resolved_options.width_ratio {
        args.push(format!("width-ratio: {}", string_literal(v)));
    }
    if let Some(v) = &resolved_options.align {
        args.push(format!("align: {}", string_literal(v.as_str())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Chunk, ChunkOptions, Position, Range};
    use insta::assert_snapshot;
    use std::path::PathBuf;

    #[test]
    fn test_format_codly_call() {
        let mut options = std::collections::HashMap::new();
        options.insert("lang-radius".to_string(), "10pt".to_string());
        options.insert("stroke".to_string(), "1pt + rgb(\"#CE412B\")".to_string());
        assert_snapshot!(format_codly_call(&options));
    }

    #[test]
    fn test_format_codly_call_empty() {
        let options = std::collections::HashMap::new();
        assert_snapshot!(format_codly_call(&options));
    }

    fn create_test_chunk(
        language: &str,
        code: &str,
        label: Option<String>,
        show: Show,
        caption: Option<String>,
    ) -> Chunk {
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        Chunk {
            index: 0, // Dummy chunk for test/inline contexts
            language: language.to_string(),
            source_language: language.to_string(),
            code: code.to_string(),
            label,
            base_indentation: String::new(),
            options: ChunkOptions {
                eval: Some(true),
                show: Some(show),
                cache: Some(true),
                caption,
                depends: vec![],
                fig_width: None,
                fig_height: None,
                dpi: None,
                fig_format: None,
                // Presentation options (use defaults for tests)
                layout: None,
                warnings_visibility: None,
                gutter: None,
                code_background: None,
                code_stroke: None,
                code_radius: None,
                code_inset: None,
                output_background: None,
                output_stroke: None,
                output_radius: None,
                output_inset: None,
                width_ratio: None,
                align: None,
                // Warning styling
                warning_background: None,
                warning_stroke: None,
                warning_radius: None,
                warning_inset: None,
            },
            codly_options: std::collections::HashMap::new(),
            errors: vec![],
            range: dummy_range.clone(),
            code_range: dummy_range,
            start_byte: 0,
            end_byte: 0,
            code_start_byte: 0,
            code_end_byte: 0,
        }
    }

    #[test]
    fn option_warnings_and_errors_keep_their_severity_even_with_show_none() {
        let backend = TypstBackend::new();
        let output = ExecutionOutput {
            result: ExecutionResult::Text("[1] 2".to_string()),
            exports: Vec::new(),
            warnings: vec![crate::executors::RuntimeWarning {
                message: "runtime".into(),
                call: None,
                line: None,
            }],
        };
        for show in [Show::Both, Show::None] {
            let mut chunk = create_test_chunk("r", "1 + 1", None, show, None);
            chunk
                .errors
                .push(crate::parser::ChunkError::warning("ignored option", None));
            chunk
                .errors
                .push(crate::parser::ChunkError::new("invalid value", None));
            let resolved = chunk.options.resolve();
            let result = backend.format_chunk(
                &chunk,
                &chunk.codly_options,
                &resolved,
                &output,
                &ChunkExecutionState::Ready,
            );
            assert!(
                result.contains(r#"errors: ([#"invalid value"],)"#),
                "{result}"
            );
            // Runtime warnings are output: `show: none` hides them.
            let warnings = if show == Show::None {
                r#"warnings: ([#"ignored option"],)"#
            } else {
                r#"warnings: ([#"ignored option"], [#"runtime"])"#
            };
            assert!(result.contains(warnings), "{result}");
        }

        let chunk = create_test_chunk("r", "1 + 1", None, Show::None, None);
        let resolved = chunk.options.resolve();
        let result = backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output,
            &ChunkExecutionState::Ready,
        );
        assert_eq!(result, "", "a clean hidden chunk renders nothing");
    }

    #[test]
    fn test_format_chunk_with_errors() {
        let backend = TypstBackend::new();
        let mut chunk = create_test_chunk("r", "1 + 1", None, Show::Both, None);
        chunk.errors.push(crate::parser::ChunkError::new(
            "Unknown option: 'foo'",
            None,
        ));
        chunk.errors.push(crate::parser::ChunkError::new(
            "Invalid value for 'eval'",
            None,
        ));
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 2".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_text_output_with_code() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, Show::Both, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 5.5".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_text_output_without_code() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 5.5".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_no_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1", None, Show::Both, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_with_name_and_caption() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk(
            "r",
            "x <- 1:10",
            Some("my-chunk".to_string()),
            Show::Both,
            Some("[My Caption]".to_string()),
        );
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_with_caption_no_name() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk(
            "r",
            "x <- 1:10",
            None,
            Show::Both,
            Some("[My Caption]".to_string()),
        );
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_plot_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10)", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Plot(PathBuf::from("/tmp/plot.svg")),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_dataframe_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "mtcars", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::DataFrame(PathBuf::from("/tmp/data.csv")),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_text_and_plot() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10); summary(1:10)", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::TextAndPlot {
                text: "Min: 1\nMax: 10".to_string(),
                plot: PathBuf::from("/tmp/plot.svg"),
            },
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_dataframe_and_plot() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(mtcars); mtcars", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::DataFrameAndPlot {
                dataframe: PathBuf::from("/tmp/data.csv"),
                plot: PathBuf::from("/tmp/plot.svg"),
            },
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_empty_name_no_figure_wrapper() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "1 + 1", Some("".to_string()), Show::Both, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 2".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_empty_text_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "invisible(1)", None, Show::Output, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_with_codly_options() {
        let backend = TypstBackend::new();
        let mut chunk = create_test_chunk("r", "x <- 1:10", None, Show::Both, None);
        chunk
            .codly_options
            .insert("stroke".to_string(), "1pt + rgb(\"#CE412B\")".to_string());
        chunk
            .codly_options
            .insert("lang-radius".to_string(), "10pt".to_string());
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_without_codly_options() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10", None, Show::Both, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        assert_snapshot!(backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready
        ));
    }

    #[test]
    fn test_format_chunk_show_replace() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "ggplot(df) + geom_line()", None, Show::Replace, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Plot(PathBuf::from("/tmp/plot.svg")),
            exports: Vec::new(),
            warnings: vec![],
        };
        let resolved = chunk.options.resolve();
        let result = backend.format_chunk(
            &chunk,
            &chunk.codly_options,
            &resolved,
            &output_data,
            &ChunkExecutionState::Ready,
        );
        assert!(
            result.contains("#knot-replace("),
            "should call #knot-replace"
        );
        assert!(
            !result.contains("#code-chunk("),
            "should not call #code-chunk"
        );
        assert!(result.contains("code:"), "should include code arg");
        assert!(result.contains("output:"), "should include output arg");
        assert!(!result.contains("layout:"), "should not include layout arg");
    }

    #[test]
    fn test_format_local_call() {
        let mut options = HashMap::new();
        options.insert("stroke".to_string(), "1pt + red".to_string());
        options.insert("lang-radius".to_string(), "5pt".to_string());

        let result = format_local_call(&options);

        assert!(result.starts_with("#local("));
        assert!(result.ends_with(")"));
        assert!(result.contains("stroke: 1pt + red"));
        assert!(result.contains("lang-radius: 5pt"));
    }
}
