use crate::executors::LanguageExecutor;
use crate::cache::{Cache, hash_dependencies};
use crate::executors::r::RExecutor;
use crate::executors::ExecutionResult;
use crate::parser::Chunk;
use crate::backend::{Backend, TypstBackend};
use anyhow::{Context, Result};
use log::info;

pub fn process_chunk(
    chunk: &Chunk,
    r_executor: &mut Option<RExecutor>,
    cache: &mut Cache,
    previous_hash: &str,
) -> Result<(String, String)> {
    let chunk_name = chunk
        .name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| format!("chunk-{}", chunk.start_byte));

    let deps_hash = hash_dependencies(&chunk.options.depends)?;

    let chunk_hash = cache.get_chunk_hash(
        &chunk.code,
        &chunk.options,
        previous_hash,
        &deps_hash,
    );

    let execution_result = if !chunk.options.eval {
        ExecutionResult::Text(String::new())
    } else if chunk.options.cache && cache.has_cached_result(&chunk_hash) {
        info!("  ✓ {} [cached]", chunk_name);
        cache.get_cached_result(&chunk_hash)?
    } else {
        info!("  ⚙️ {} [executing]", chunk_name);
        let result = match chunk.language.as_str() {
            "r" => r_executor
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
                chunk.start_byte, // Use start_byte as unique ID for now
                chunk.name.clone(),
                chunk_hash.clone(),
                &result,
                chunk.options.depends.clone(),
            )?;
        }
        result
    };

    let backend = TypstBackend::new();
    let chunk_output_final = backend.format_chunk(chunk, &execution_result);
    
    Ok((chunk_output_final, chunk_hash))
}
