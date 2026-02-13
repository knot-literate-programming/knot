use crate::executors::{ExecutionOutput, ExecutionResult};
use crate::parser::{Chunk, Layout, ResolvedChunkOptions, Show};
use std::collections::HashMap;

/// Formats a HashMap of codly options into a Typst #codly() function call.
pub fn format_codly_call(options: &HashMap<String, String>) -> String {
    let args: Vec<String> = options
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value))
        .collect();
    format!("#codly({})", args.join(", "))
}

/// Formats a HashMap of codly options into a Typst #local() function call.
pub fn format_local_call(options: &HashMap<String, String>) -> String {
    let args: Vec<String> = options
        .iter()
        .map(|(key, value)| format!("{}: {}", key, value))
        .collect();
    format!("#local({})", args.join(", "))
}

pub trait Backend {
    /// Formats a processed chunk into the target document syntax.
    fn format_chunk(
        &self,
        chunk: &Chunk,
        resolved_options: &ResolvedChunkOptions,
        output: &ExecutionOutput,
        is_inert: bool,
    ) -> String;
}

pub struct TypstBackend;

impl Default for TypstBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TypstBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Backend for TypstBackend {
    fn format_chunk(
        &self,
        chunk: &Chunk,
        resolved_options: &ResolvedChunkOptions,
        output: &ExecutionOutput,
        is_inert: bool,
    ) -> String {
        // If show is none, return empty string immediately
        if matches!(resolved_options.show, Show::None) {
            return String::new();
        }

        let mut args = vec![];
        // For inert chunks, we keep the original language for syntax highlighting
        args.push(format!("lang: \"{}\"", chunk.language));
        if let Some(name) = &chunk.name {
            args.push(format!("name: \"{}\"", name));
        } else {
            // Only pass caption to code-chunk if there is no name wrapper (no figure)
            if let Some(caption) = &chunk.options.caption {
                args.push(format!("caption: [{}]", caption));
            }
        }

        if is_inert {
            args.push("is-inert: true".to_string());
        }

        // Include errors as a special argument to code-chunk (if any)
        if !chunk.errors.is_empty() {
            let mut error_list = chunk
                .errors
                .iter()
                .map(|e| format!("[{}]", e.message))
                .collect::<Vec<_>>()
                .join(", ");

            // In Typst, (item,) is a single-element array.
            if chunk.errors.len() == 1 {
                error_list.push(',');
            }
            args.push(format!("errors: ({})", error_list));
        }

        // Include runtime warnings based on visibility setting
        if !output.warnings.is_empty() {
            match resolved_options.warnings_visibility {
                crate::parser::WarningsVisibility::None => {
                    // Suppress warnings entirely — don't add to args
                }
                visibility => {
                    let mut warning_list = output
                        .warnings
                        .iter()
                        .map(|w| format!("[{}]", w.message))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if output.warnings.len() == 1 {
                        warning_list.push(',');
                    }
                    args.push(format!("warnings: ({})", warning_list));
                    if matches!(visibility, crate::parser::WarningsVisibility::Inline) {
                        args.push("warnings-position: \"inline\"".to_string());
                    }
                }
            }
        }

        // Generate code based on show option
        let should_show_code = matches!(resolved_options.show, Show::Both | Show::Code);

        if should_show_code {
            // Use #local() for chunk-specific codly options (local scope)
            let code_str = if !chunk.codly_options.is_empty() {
                let local_call = format_local_call(&chunk.codly_options);
                format!(
                    "[{}[```{}\n{}```]]",
                    local_call,
                    chunk.language,
                    chunk.code.trim()
                )
            } else {
                format!("[```{}\n{}```]", chunk.language, chunk.code.trim())
            };
            args.push(format!("code: {}", code_str));
        } else {
            args.push("code: none".to_string());
        }

        // Generate output based on show option
        let should_show_output = matches!(resolved_options.show, Show::Both | Show::Output);

        if should_show_output {
            let output_str = match &output.result {
                ExecutionResult::Text(text) if !text.trim().is_empty() => {
                    format!("[```output\n{}```]", text.trim())
                }
                ExecutionResult::Plot(path) => {
                    let abs_plot = path.canonicalize().unwrap_or_else(|_| path.clone());
                    format!("[#image(\"{}\")]", abs_plot.to_string_lossy())
                }
                ExecutionResult::DataFrame(csv_path) => {
                    let abs_csv = csv_path.canonicalize().unwrap_or_else(|_| csv_path.clone());
                    format!(
                        "[#{{ let data = csv(\"{}\"); table(columns: data.first().len(), ..data.flatten()) }}]",
                        abs_csv.to_string_lossy()
                    )
                }
                ExecutionResult::TextAndPlot { text, plot } => {
                    let abs_plot = plot.canonicalize().unwrap_or_else(|_| plot.clone());
                    format!(
                        "[#image(\"{}\")\n```output\n{}```]",
                        abs_plot.to_string_lossy(),
                        text.trim()
                    )
                }
                ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
                    let abs_csv = dataframe
                        .canonicalize()
                        .unwrap_or_else(|_| dataframe.clone());
                    let abs_plot = plot.canonicalize().unwrap_or_else(|_| plot.clone());
                    format!(
                        "[#{{ let data = csv(\"{}\"); table(columns: data.first().len(), ..data.flatten()) }}\n#image(\"{}\")]",
                        abs_csv.to_string_lossy(),
                        abs_plot.to_string_lossy()
                    )
                }
                _ => "none".to_string(),
            };
            args.push(format!("output: {}", output_str));
        } else {
            args.push("output: none".to_string());
        }

        // Add presentation options
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

        if let Some(code_bg) = &resolved_options.code_background {
            args.push(format!("code-background: {}", code_bg));
        }
        if let Some(code_stroke) = &resolved_options.code_stroke {
            args.push(format!("code-stroke: {}", code_stroke));
        }
        if let Some(code_radius) = &resolved_options.code_radius {
            args.push(format!("code-radius: {}", code_radius));
        }
        if let Some(code_inset) = &resolved_options.code_inset {
            args.push(format!("code-inset: {}", code_inset));
        }

        if let Some(output_bg) = &resolved_options.output_background {
            args.push(format!("output-background: {}", output_bg));
        }
        if let Some(output_stroke) = &resolved_options.output_stroke {
            args.push(format!("output-stroke: {}", output_stroke));
        }
        if let Some(output_radius) = &resolved_options.output_radius {
            args.push(format!("output-radius: {}", output_radius));
        }
        if let Some(output_inset) = &resolved_options.output_inset {
            args.push(format!("output-inset: {}", output_inset));
        }

        if let Some(warning_bg) = &resolved_options.warning_background {
            args.push(format!("warning-background: {}", warning_bg));
        }
        if let Some(warning_stroke) = &resolved_options.warning_stroke {
            args.push(format!("warning-stroke: {}", warning_stroke));
        }
        if let Some(warning_radius) = &resolved_options.warning_radius {
            args.push(format!("warning-radius: {}", warning_radius));
        }
        if let Some(warning_inset) = &resolved_options.warning_inset {
            args.push(format!("warning-inset: {}", warning_inset));
        }

        if let Some(width_ratio) = &resolved_options.width_ratio {
            args.push(format!("width-ratio: \"{}\"", width_ratio));
        }
        if let Some(align) = &resolved_options.align {
            args.push(format!("align: \"{}\"", align));
        }

        let code_chunk_call = format!("#code-chunk({})", args.join(", "));
        let mut chunk_output_final = String::new();

        if let Some(name) = &chunk.name {
            if !name.trim().is_empty() {
                let mut figure_named_args = vec![];
                figure_named_args.push("kind: raw".to_string());
                figure_named_args.push("supplement: \"Chunk\"".to_string());

                if let Some(caption) = &chunk.options.caption {
                    figure_named_args.push(format!("caption: [{}]", caption));
                }

                let figure_call_start = format!("#figure({})", figure_named_args.join(", "));

                chunk_output_final.push_str(&figure_call_start);
                chunk_output_final.push_str(&format!(
                    "[{}]
",
                    code_chunk_call
                ));
                // Add label
                chunk_output_final.push_str(&format!(" <{}>", name.trim()));
            } else {
                chunk_output_final.push_str(&code_chunk_call);
            }
        } else {
            chunk_output_final.push_str(&code_chunk_call);
        }

        chunk_output_final
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Chunk, ChunkOptions, Position, Range};
    use std::path::PathBuf;

    #[test]
    fn test_format_codly_call() {
        let mut options = std::collections::HashMap::new();
        options.insert("lang-radius".to_string(), "10pt".to_string());
        options.insert("stroke".to_string(), "1pt + rgb(\"#CE412B\")".to_string());

        let result = format_codly_call(&options);

        // Check that it starts with #codly(
        assert!(result.starts_with("#codly("));
        assert!(result.ends_with(")"));

        // Check that both options are present
        assert!(result.contains("lang-radius: 10pt"));
        assert!(result.contains("stroke: 1pt + rgb(\"#CE412B\")"));
    }

    #[test]
    fn test_format_codly_call_empty() {
        let options = std::collections::HashMap::new();
        let result = format_codly_call(&options);
        assert_eq!(result, "#codly()");
    }

    fn create_test_chunk(
        language: &str,
        code: &str,
        name: Option<String>,
        echo: bool,
        output: bool,
        caption: Option<String>,
    ) -> Chunk {
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        Chunk {
            language: language.to_string(),
            code: code.to_string(),
            name,
            options: ChunkOptions {
                eval: Some(true),
                show: Some(match (echo, output) {
                    (true, true) => Show::Both,
                    (true, false) => Show::Code,
                    (false, true) => Show::Output,
                    (false, false) => Show::Output, // fallback to output
                }),
                cache: Some(true),
                caption,
                depends: vec![],
                fig_width: None,
                fig_height: None,
                dpi: None,
                fig_format: None,
                constant: vec![],
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
    fn test_format_chunk_with_errors() {
        let backend = TypstBackend::new();
        let mut chunk = create_test_chunk("r", "1 + 1", None, true, true, None);
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
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("errors: ([Unknown option: 'foo'], [Invalid value for 'eval'])"));
    }

    #[test]
    fn test_format_chunk_text_output_with_echo() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, true, true, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 5.5".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("lang: \"r\""));
        assert!(output.contains("code: [```r\nx <- 1:10\nmean(x)```]"));
        assert!(output.contains("output: [```output\n[1] 5.5```]"));
        assert!(output.starts_with("#code-chunk("));
    }

    #[test]
    fn test_format_chunk_text_output_without_echo() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, false, true, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 5.5".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("code: none"));
        assert!(output.contains("output: [```output\n[1] 5.5```]"));
    }

    #[test]
    fn test_format_chunk_no_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1", None, true, false, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("output: none"));
    }

    #[test]
    fn test_format_chunk_with_name_and_caption() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk(
            "r",
            "x <- 1:10",
            Some("my-chunk".to_string()),
            true,
            true,
            Some("[My Caption]".to_string()),
        );
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("#figure("));
        assert!(output.contains("kind: raw"));
        assert!(output.contains("supplement: \"Chunk\""));
        assert!(output.contains("caption: [[My Caption]]"));
        assert!(output.contains("<my-chunk>"));
    }

    #[test]
    fn test_format_chunk_with_caption_no_name() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk(
            "r",
            "x <- 1:10",
            None,
            true,
            true,
            Some("[My Caption]".to_string()),
        );
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        // Caption should be passed directly to code-chunk when no name
        assert!(output.contains("caption: [[My Caption]]"));
        assert!(!output.contains("#figure("));
    }

    #[test]
    fn test_format_chunk_plot_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10)", None, false, true, None);
        let plot_path = PathBuf::from("/tmp/plot.svg");
        let output_data = ExecutionOutput {
            result: ExecutionResult::Plot(plot_path.clone()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("output: [#image("));
        assert!(output.contains("plot.svg"));
    }

    #[test]
    fn test_format_chunk_dataframe_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "mtcars", None, false, true, None);
        let csv_path = PathBuf::from("/tmp/data.csv");
        let output_data = ExecutionOutput {
            result: ExecutionResult::DataFrame(csv_path.clone()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("output: [#{ let data = csv("));
        assert!(output.contains("data.csv"));
        assert!(output.contains("table(columns: data.first().len()"));
    }

    #[test]
    fn test_format_chunk_text_and_plot() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10); summary(1:10)", None, false, true, None);
        let plot_path = PathBuf::from("/tmp/plot.svg");
        let output_data = ExecutionOutput {
            result: ExecutionResult::TextAndPlot {
                text: "Min: 1\nMax: 10".to_string(),
                plot: plot_path.clone(),
            },
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("#image("));
        assert!(output.contains("plot.svg"));
        assert!(output.contains("Min: 1"));
        assert!(output.contains("Max: 10"));
    }

    #[test]
    fn test_format_chunk_dataframe_and_plot() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(mtcars); mtcars", None, false, true, None);
        let csv_path = PathBuf::from("/tmp/data.csv");
        let plot_path = PathBuf::from("/tmp/plot.svg");
        let output_data = ExecutionOutput {
            result: ExecutionResult::DataFrameAndPlot {
                dataframe: csv_path.clone(),
                plot: plot_path.clone(),
            },
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        assert!(output.contains("let data = csv("));
        assert!(output.contains("data.csv"));
        assert!(output.contains("#image("));
        assert!(output.contains("plot.svg"));
    }

    #[test]
    fn test_format_chunk_empty_name_no_figure_wrapper() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "1 + 1", Some("".to_string()), true, true, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 2".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        // Empty name should not create figure wrapper
        assert!(!output.contains("#figure("));
        assert!(output.starts_with("#code-chunk("));
    }

    #[test]
    fn test_format_chunk_empty_text_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "invisible(1)", None, false, true, None);
        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        // Empty text should result in output: none
        assert!(output.contains("output: none"));
    }

    #[test]
    fn test_format_chunk_with_codly_options() {
        let backend = TypstBackend::new();
        let mut chunk = create_test_chunk("r", "x <- 1:10", None, true, true, None);

        // Add codly options
        chunk
            .codly_options
            .insert("stroke".to_string(), "1pt + rgb(\"#CE412B\")".to_string());
        chunk
            .codly_options
            .insert("lang-radius".to_string(), "10pt".to_string());

        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        // Should use #local() for chunk-specific codly options
        assert!(output.contains("#local("));
        assert!(output.contains("stroke: 1pt + rgb(\"#CE412B\")"));
        assert!(output.contains("lang-radius: 10pt"));

        // Code block should follow the #local() call
        assert!(output.contains("```r"));
    }

    #[test]
    fn test_format_chunk_without_codly_options() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10", None, true, true, None);

        let output_data = ExecutionOutput {
            result: ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string()),
            warnings: vec![],
            error: None,
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &output_data, false);

        // Without codly options, should NOT have #local()
        assert!(!output.contains("#local("));

        // Should have simple code
        assert!(output.contains("code: [```r"));
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
