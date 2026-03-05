//! Chunk option resolution and hash computation for the planning phase.
#![allow(missing_docs)]

use crate::cache::{hash_dependencies, hashing};
use crate::compiler::pipeline::ChunkExecutionState;
use crate::config::Config;
use crate::parser::ast::Chunk;
use crate::parser::{ChunkOptions, ResolvedChunkOptions};
use anyhow::Result;
use std::collections::HashMap;

/// Applies config layering (global → language → error) to produce resolved options
/// and merged codly options for a chunk.
pub(super) fn resolve_options(
    chunk: &Chunk,
    config: &Config,
    state: &ChunkExecutionState,
) -> (ChunkOptions, ResolvedChunkOptions, HashMap<String, String>) {
    let mut chunk_options = chunk.options.clone();

    let mut effective_defaults = config.chunk_defaults.clone();
    if let Some(lang_defaults) = config.get_language_defaults(&chunk.language) {
        effective_defaults.merge(lang_defaults);
    }
    if matches!(state, ChunkExecutionState::Inert)
        && let Some(error_defaults) = config.get_language_error_defaults(&chunk.language)
    {
        effective_defaults.merge(error_defaults);
    }

    chunk_options.apply_config_defaults(&effective_defaults);

    let mut merged_codly_options = effective_defaults.codly_options.clone();
    for (key, value) in &chunk.codly_options {
        merged_codly_options.insert(key.clone(), value.clone());
    }

    let resolved_options = chunk_options.resolve();
    (chunk_options, resolved_options, merged_codly_options)
}

/// Computes the chunk hash from code, options, previous hash, and file dependencies.
///
/// Freeze objects are intentionally excluded from the hash: their immutability
/// is enforced by the snapshot mechanism, and cache invalidation propagates
/// correctly through hash chaining (`previous_hash`).
pub(super) fn compute_hash(
    code: &str,
    chunk_options: &ChunkOptions,
    previous_hash: &str,
) -> Result<String> {
    let deps_hash = hash_dependencies(&chunk_options.depends)?;
    Ok(hashing::get_chunk_hash(
        code,
        chunk_options,
        previous_hash,
        &deps_hash,
    ))
}
