//! Chunk option resolution and hash computation for the planning phase.
#![allow(missing_docs)]

use crate::cache::{hash_dependencies, hashing};
use crate::compiler::pipeline::ChunkExecutionState;
use crate::config::Config;
use crate::parser::ast::Chunk;
use crate::parser::{ChunkError, ChunkOptions, ResolvedChunkOptions};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
/// Dependencies resolve against the project root, independent of process cwd.
pub(super) fn compute_hash(
    language: &str,
    code: &str,
    chunk_options: &ChunkOptions,
    previous_hash: &str,
    project_root: &std::path::Path,
) -> Result<String> {
    let dependencies: Vec<_> = chunk_options
        .depends
        .iter()
        .map(|path| project_root.join(path))
        .collect();
    let deps_hash = hash_dependencies(&dependencies)?;
    Ok(hashing::get_chunk_hash(
        language,
        code,
        chunk_options,
        previous_hash,
        &deps_hash,
    ))
}

/// Errors for the `depends` files of a chunk that do not exist, after applying
/// the `knot.toml` defaults. The compiler rejects such a chunk (its error block
/// is shown in the PDF) and the LSP reports the same messages.
pub fn dependency_errors(
    chunk: &Chunk,
    config: &Config,
    project_root: &Path,
    source: &str,
) -> Vec<ChunkError> {
    let (options, _, _) = resolve_options(chunk, config, &ChunkExecutionState::Ready);
    missing_dependencies(chunk, &options.depends, project_root, source)
}

/// Errors for the files of `depends` missing under `project_root`, on the
/// chunk's `#| depends:` line when it has one (otherwise from `knot.toml`).
pub(super) fn missing_dependencies(
    chunk: &Chunk,
    depends: &[PathBuf],
    project_root: &Path,
    source: &str,
) -> Vec<ChunkError> {
    let line_offset = source
        .get(chunk.start_byte..chunk.end_byte)
        .and_then(|text| {
            text.lines().position(|line| {
                line.trim_start()
                    .strip_prefix("#|")
                    .is_some_and(|rest| rest.trim_start().starts_with("depends"))
            })
        });
    depends
        .iter()
        .filter(|path| !project_root.join(path).exists())
        .map(|path| ChunkError::new(missing_dependency_message(path), line_offset))
        .collect()
}

/// Message for a `depends` file that does not exist.
pub fn missing_dependency_message(path: &Path) -> String {
    format!(
        "Dependency '{}' not found (depends paths are relative to the project root). The chunk is not executed and later chunks in this language are suspended.",
        path.display()
    )
}
