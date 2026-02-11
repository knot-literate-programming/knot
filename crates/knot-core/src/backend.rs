use crate::executors::ExecutionResult;
use crate::parser::{Chunk, ResolvedChunkOptions};

pub trait Backend {
    /// Formats a processed chunk into the target document syntax.
    fn format_chunk(
        &self,
        chunk: &Chunk,
        resolved_options: &ResolvedChunkOptions,
        result: &ExecutionResult,
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
        result: &ExecutionResult,
    ) -> String {
        let mut args = vec![];
        args.push(format!("lang: \"{}\"", chunk.language));
        if let Some(name) = &chunk.name {
            args.push(format!("name: \"{}\"", name));
        } else {
            // Only pass caption to code-chunk if there is no name wrapper (no figure)
            if let Some(caption) = &chunk.options.caption {
                args.push(format!("caption: [{}]", caption));
            }
        }

        args.push(format!("echo: {}", resolved_options.echo));

        // Include errors as a special argument to code-chunk (if any)
        if !chunk.errors.is_empty() {
            let error_list = chunk
                .errors
                .iter()
                .map(|e| format!("\"{}\"", e.message.replace('\"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", ");
            args.push(format!("errors: ({})", error_list));
        }

        if resolved_options.echo {
            let input_str = format!("[```{}\n{}```]", chunk.language, chunk.code.trim());
            args.push(format!("input: {}", input_str));
        } else {
            args.push("input: none".to_string());
        }

        if resolved_options.output {
            let output_str = match result {
                ExecutionResult::Text(text) if !text.trim().is_empty() => {
                    format!("[```\n{}```]", text.trim())
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
                        "[#image(\"{}\")\n```\n{}```]",
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
        args.push(format!("layout: \"{}\"", resolved_options.layout));

        if let Some(gutter) = &resolved_options.gutter {
            args.push(format!("gutter: {}", gutter));
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

        if let Some(width_ratio) = &resolved_options.width_ratio {
            args.push(format!("width-ratio: \"{}\"", width_ratio));
        }
        if let Some(align) = &resolved_options.align {
            args.push(format!("align: \"{}\"", align));
        }

        let code_chunk_call = format!("#code-chunk({})", args.join(", "));
        let mut chunk_output_final = String::new();

        // Add codly() call if there are codly options from the chunk
        if !chunk.codly_options.is_empty() {
            let codly_args: Vec<String> = chunk
                .codly_options
                .iter()
                .map(|(key, value)| format!("{}: {}", key, value))
                .collect();
            chunk_output_final.push_str(&format!("#codly({})\n", codly_args.join(", ")));
        }

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
    use crate::parser::{Chunk, ChunkOptions, Position, Range};
    use std::path::PathBuf;

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
                echo: Some(echo),
                output: Some(output),
                cache: Some(true),
                caption,
                depends: vec![],
                label: None,
                fig_width: None,
                fig_height: None,
                dpi: None,
                fig_format: None,
                fig_alt: None,
                constant: vec![],
                // Presentation options (use defaults for tests)
                layout: None,
                gutter: None,
                output_background: None,
                output_stroke: None,
                output_radius: None,
                output_inset: None,
                width_ratio: None,
                align: None,
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
        let result = ExecutionResult::Text("[1] 2".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(
            output.contains("errors: (\"Unknown option: 'foo'\", \"Invalid value for 'eval'\")")
        );
    }

    #[test]
    fn test_format_chunk_text_output_with_echo() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, true, true, None);
        let result = ExecutionResult::Text("[1] 5.5".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(output.contains("lang: \"r\""));
        assert!(output.contains("echo: true"));
        assert!(output.contains("input: [```r\nx <- 1:10\nmean(x)```]"));
        assert!(output.contains("output: [```\n[1] 5.5```]"));
        assert!(output.starts_with("#code-chunk("));
    }

    #[test]
    fn test_format_chunk_text_output_without_echo() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1:10\nmean(x)", None, false, true, None);
        let result = ExecutionResult::Text("[1] 5.5".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(output.contains("echo: false"));
        assert!(output.contains("input: none"));
        assert!(output.contains("output: [```\n[1] 5.5```]"));
    }

    #[test]
    fn test_format_chunk_no_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "x <- 1", None, true, false, None);
        let result = ExecutionResult::Text("".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

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
        let result = ExecutionResult::Text("[1] 1  2  3  4  5  6  7  8  9 10".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

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
        let result = ExecutionResult::Text("".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        // Caption should be passed directly to code-chunk when no name
        assert!(output.contains("caption: [[My Caption]]"));
        assert!(!output.contains("#figure("));
    }

    #[test]
    fn test_format_chunk_plot_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10)", None, false, true, None);
        let plot_path = PathBuf::from("/tmp/plot.svg");
        let result = ExecutionResult::Plot(plot_path.clone());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(output.contains("output: [#image("));
        assert!(output.contains("plot.svg"));
    }

    #[test]
    fn test_format_chunk_dataframe_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "mtcars", None, false, true, None);
        let csv_path = PathBuf::from("/tmp/data.csv");
        let result = ExecutionResult::DataFrame(csv_path.clone());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(output.contains("output: [#{ let data = csv("));
        assert!(output.contains("data.csv"));
        assert!(output.contains("table(columns: data.first().len()"));
    }

    #[test]
    fn test_format_chunk_text_and_plot() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "plot(1:10); summary(1:10)", None, false, true, None);
        let plot_path = PathBuf::from("/tmp/plot.svg");
        let result = ExecutionResult::TextAndPlot {
            text: "Min: 1\nMax: 10".to_string(),
            plot: plot_path.clone(),
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

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
        let result = ExecutionResult::DataFrameAndPlot {
            dataframe: csv_path.clone(),
            plot: plot_path.clone(),
        };
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        assert!(output.contains("let data = csv("));
        assert!(output.contains("data.csv"));
        assert!(output.contains("#image("));
        assert!(output.contains("plot.svg"));
    }

    #[test]
    fn test_format_chunk_empty_name_no_figure_wrapper() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "1 + 1", Some("".to_string()), true, true, None);
        let result = ExecutionResult::Text("[1] 2".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        // Empty name should not create figure wrapper
        assert!(!output.contains("#figure("));
        assert!(output.starts_with("#code-chunk("));
    }

    #[test]
    fn test_format_chunk_empty_text_output() {
        let backend = TypstBackend::new();
        let chunk = create_test_chunk("r", "invisible(1)", None, false, true, None);
        let result = ExecutionResult::Text("".to_string());
        let resolved = chunk.options.resolve();

        let output = backend.format_chunk(&chunk, &resolved, &result);

        // Empty text should result in output: none
        assert!(output.contains("output: none"));
    }
}
