use crate::parser::Chunk;
use crate::executors::ExecutionResult;

pub trait Backend {
    /// Formats a processed chunk into the target document syntax.
    fn format_chunk(&self, chunk: &Chunk, result: &ExecutionResult) -> String;
}

pub struct TypstBackend;

impl TypstBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Backend for TypstBackend {
    fn format_chunk(&self, chunk: &Chunk, result: &ExecutionResult) -> String {
        let mut args = vec![];
        args.push(format!("lang: \"{}\"", chunk.language));
        if let Some(name) = &chunk.name {
            args.push(format!("name: \"{}\"", name));
        } else {
            // Only pass caption to code-chunk if there is no name wrapper (no figure)
            if let Some(caption) = &chunk.options.caption {
                 args.push(format!("caption: {}", caption));
            }
        }
        
        args.push(format!("echo: {}", chunk.options.echo));

        if chunk.options.echo {
            let input_str = format!("[```{}\n{}```]", chunk.language, chunk.code.trim());
            args.push(format!("input: {}", input_str));
        } else {
            args.push("input: none".to_string());
        }

        if chunk.options.output {
            let output_str = match result {
                ExecutionResult::Text(text) if !text.trim().is_empty() => {
                    format!("[```\n{}```]", text.trim())
                }
                ExecutionResult::Plot(path) => {
                    let abs_plot = path.canonicalize() 
                        .unwrap_or_else(|_| path.clone());
                    format!("[#image(\"{}\")]", abs_plot.to_string_lossy())
                }
                ExecutionResult::DataFrame(csv_path) => {
                    let abs_csv = csv_path.canonicalize() 
                        .unwrap_or_else(|_| csv_path.clone());
                    format!("[#{{ let data = csv(\"{}\"); table(columns: data.first().len(), ..data.flatten()) }}]", abs_csv.to_string_lossy())
                }
                ExecutionResult::TextAndPlot { text, plot } => {
                    let abs_plot = plot.canonicalize() 
                        .unwrap_or_else(|_| plot.clone());
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
                    let abs_plot = plot
                        .canonicalize() 
                        .unwrap_or_else(|_| plot.clone());
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

        let code_chunk_call = format!("#code-chunk({})", args.join(", "));
        let mut chunk_output_final = String::new();

        if let Some(name) = &chunk.name {
            if !name.trim().is_empty() {
                let mut figure_named_args = vec![];
                figure_named_args.push("kind: raw".to_string());
                figure_named_args.push("supplement: \"Chunk\"".to_string());

                if let Some(caption) = &chunk.options.caption {
                    figure_named_args.push(format!("caption: {}", caption));
                }

                let figure_call_start = format!("#figure({})", figure_named_args.join(", "));
                
                chunk_output_final.push_str(&figure_call_start);
                chunk_output_final.push_str(&format!("[{}]
", code_chunk_call));
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
