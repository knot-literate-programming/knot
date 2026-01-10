use crate::cache::{hash_dependencies, Cache};
use crate::codegen::CodeGenerator;
use crate::executors::r::RExecutor;
use crate::executors::{ExecutionResult, LanguageExecutor};
use crate::parser::Document;
use crate::get_cache_dir;
use anyhow::{Context, Result};

// From section 3.1 and 6.1 (Semaine 2) of the reference document

pub struct Compiler {
    r_executor: Option<RExecutor>,
    // In the future, we'll have more executors
    // lilypond_executor: Option<LilypondExecutor>,
    // python_executor: Option<PythonExecutor>,
}

impl Compiler {
    pub fn new() -> Result<Self> {
        let cache_dir = get_cache_dir();
        let r_executor = RExecutor::new(cache_dir).context("Failed to initialize R executor")?;

        Ok(Self {
            r_executor: Some(r_executor),
        })
    }

    /// Compiles a document by executing its code chunks and generating a new Typst source file.
    pub fn compile(&mut self, doc: &Document) -> Result<String> {
        let mut codegen = CodeGenerator::new();
        let cache_dir = get_cache_dir();
        let mut cache = Cache::new(cache_dir)?;
        let mut previous_hash = String::new();

        if let Some(ref mut r_exec) = self.r_executor {
            r_exec.initialize()?;
        }

        println!("🔧 Processing {} code chunks...", doc.chunks.len());

        for (index, chunk) in doc.chunks.iter().enumerate() {
            let chunk_name = chunk
                .name
                .as_deref()
                .map(String::from)
                .unwrap_or_else(|| format!("chunk-{}", index));

            let deps_hash = hash_dependencies(&chunk.options.depends)?;

            let chunk_hash = cache.get_chunk_hash(
                &chunk.code,
                &chunk.options,
                &previous_hash,
                &deps_hash,
            );

            let result = if !chunk.options.eval {
                ExecutionResult::Text(String::new())
            } else if chunk.options.cache && cache.has_cached_result(&chunk_hash) {
                println!("  ✓ {} [cached]", chunk_name);
                cache.get_cached_result(&chunk_hash)?
            } else {
                println!("  ⚙️ {} [executing]", chunk_name);
                let result = match chunk.language.as_str() {
                    "r" => self
                        .r_executor
                        .as_mut()
                        .context("R executor not initialized")?
                        .execute(&chunk.code)?,
                    _ => ExecutionResult::Text(format!(
                        "Language '{}' not supported yet.",
                        chunk.language
                    )),
                };
                if chunk.options.cache {
                    cache.save_result(
                        index,
                        chunk.name.clone(),
                        chunk_hash.clone(),
                        &result,
                        chunk.options.depends.clone(),
                    )?;
                }
                result
            };

            // Propagate hash for chaining
            previous_hash = chunk_hash;

            let mut args = vec![];
            args.push(format!("lang: \"{}\"", chunk.language));
            if let Some(name) = &chunk.name {
                args.push(format!("name: \"{}\"", name));
            }
            if let Some(caption) = &chunk.options.caption {
                args.push(format!("caption: {}", caption));
            }
            args.push(format!("echo: {}", chunk.options.echo));
            args.push(format!("eval: {}", chunk.options.eval));

            if chunk.options.echo {
                let input_str = format!("[```{}\n{}```]", chunk.language, chunk.code.trim());
                args.push(format!("input: {}", input_str));
            } else {
                args.push("input: none".to_string());
            }

            if chunk.options.output {
                let output_str = match &result {
                    ExecutionResult::Text(text) if !text.trim().is_empty() => {
                        format!("[```\n{}```]", text.trim())
                    }
                    ExecutionResult::Plot(path) => {
                        format!("[#image(\"{}\")]", path.to_string_lossy())
                    }
                    ExecutionResult::Both { text, plot } => {
                        format!(
                            "[#image(\"{}\")\n```\n{}```]",
                            plot.to_string_lossy(),
                            text.trim()
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
                    chunk_output_final.push_str(&format!("[{}]", code_chunk_call));
                    chunk_output_final.push_str(&format!(" <{}>", name.trim()));
                } else {
                    chunk_output_final.push_str(&code_chunk_call);
                }
            } else {
                chunk_output_final.push_str(&code_chunk_call);
            }

            codegen.add_chunk_result(chunk_output_final);
        }

        println!("✓ All chunks processed.");
        codegen.generate(doc)
    }
}
