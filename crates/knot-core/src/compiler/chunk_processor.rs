//! Code Chunk Processing
//!
//! This module orchestrates the lifecycle of a code chunk during compilation:
//! 1. Resolve chunk options with global defaults.
//! 2. Calculate chunk hash (including code, options, dependencies, and constants).
//! 3. Check cache for previous results.
//! 4. Execute code via the appropriate language executor if not cached.
//! 5. Save results to cache if enabled.
//! 6. Format the output using the Typst backend.

use crate::backend::{Backend, TypstBackend};
use crate::cache::{Cache, hash_dependencies, hashing};
use crate::compiler::inline_processor;
use crate::compiler::snapshot_manager::SnapshotManager;
use crate::config::Config;
use crate::executors::{ExecutionOutput, ExecutionResult, ExecutorManager, GraphicsOptions};
use crate::parser::{Chunk, ChunkOptions, InlineExpr, ResolvedChunkOptions};
use anyhow::Result;
use log::info;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

/// Execution lifecycle state of a chunk within the compilation pipeline.
///
/// - `Ready`   : cache valid or execution just succeeded — full output available.
/// - `Inert`   : follows an upstream error; rendered as raw code without execution.
/// - `Pending` : cache invalidated, not yet executed (reserved for progressive
///   compilation: first-pass placeholder before execution completes).
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkExecutionState {
    Ready,
    Inert,
    Pending,
}

/// Per-chunk immutable data forwarded to `try_execute`.
struct ExecutionRequest<'a> {
    chunk: &'a Chunk,
    chunk_hash: &'a str,
    chunk_name: &'a str,
    previous_hash: &'a str,
    resolved_options: &'a ResolvedChunkOptions,
    chunk_options: &'a ChunkOptions,
    state: &'a ChunkExecutionState,
}

/// Stateful processor that handles chunk and inline expression execution for one compilation pass.
///
/// Holds mutable references to the three resources that change during compilation
/// (`executor_manager`, `cache`, `snapshot_manager`) and immutable references to the
/// configuration that stays fixed (`config`, `backend`, `project_root`).
pub struct ChunkProcessor<'a> {
    executor_manager: &'a mut ExecutorManager,
    cache: &'a mut Cache,
    snapshot_manager: &'a mut SnapshotManager,
    config: &'a Config,
    backend: &'a TypstBackend,
    project_root: &'a Path,
}

impl<'a> ChunkProcessor<'a> {
    pub fn new(
        executor_manager: &'a mut ExecutorManager,
        cache: &'a mut Cache,
        snapshot_manager: &'a mut SnapshotManager,
        config: &'a Config,
        backend: &'a TypstBackend,
        project_root: &'a Path,
    ) -> Self {
        Self {
            executor_manager,
            cache,
            snapshot_manager,
            config,
            backend,
            project_root,
        }
    }

    /// Processes a code chunk: resolves options, checks cache, executes if needed, formats output.
    ///
    /// Returns `(typst_output, chunk_hash)`.
    pub fn process_chunk(
        &mut self,
        chunk: &Chunk,
        state: ChunkExecutionState,
        previous_hash: &str,
    ) -> Result<(String, String)> {
        let (chunk_options, resolved_options, merged_codly_options) =
            resolve_options(chunk, self.config, &state);

        let chunk_name = chunk
            .name
            .as_deref()
            .map(String::from)
            .unwrap_or_else(|| format!("chunk-{}", chunk.index));

        let chunk_hash = compute_hash(
            &chunk.code,
            &chunk_options,
            previous_hash,
            self.executor_manager,
            &chunk.language,
        )?;

        if resolved_options.cache && self.cache.has_cached_result(&chunk_hash) {
            info!("  ✓ {} [cached]", chunk_name);
            let execution_output = self.cache.get_cached_result(&chunk_hash)?;
            if let Some(error) = &execution_output.error {
                return Err(anyhow::anyhow!("{}", error));
            }
            let output = format_output(
                self.backend,
                chunk,
                &merged_codly_options,
                &resolved_options,
                &execution_output,
                &state,
            );
            return Ok((output, chunk_hash));
        }

        let req = ExecutionRequest {
            chunk,
            chunk_hash: &chunk_hash,
            chunk_name: &chunk_name,
            previous_hash,
            resolved_options: &resolved_options,
            chunk_options: &chunk_options,
            state: &state,
        };
        let execution_output = self.try_execute(&req)?;

        if let Some(error) = &execution_output.error {
            return Err(anyhow::anyhow!("{}", error));
        }

        let output = format_output(
            self.backend,
            chunk,
            &merged_codly_options,
            &resolved_options,
            &execution_output,
            &state,
        );
        Ok((output, chunk_hash))
    }

    /// Processes an inline expression, delegating to the inline processor.
    ///
    /// Returns `(typst_output, inline_hash)`.
    pub fn process_inline(
        &mut self,
        inline_expr: &InlineExpr,
        previous_hash: &str,
    ) -> Result<(String, String)> {
        inline_processor::process_inline_expr(
            inline_expr,
            self.executor_manager,
            self.cache,
            previous_hash,
        )
    }

    /// Executes the chunk (or produces empty output if inert/eval=false), then caches the result.
    fn try_execute(&mut self, req: &ExecutionRequest<'_>) -> Result<ExecutionOutput> {
        if matches!(req.state, ChunkExecutionState::Inert) || !req.resolved_options.eval {
            return Ok(ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
                error: None,
            });
        }

        info!("  ⚙️ {} [executing]", req.chunk_name);

        // Lazy state restoration: only restore when we actually need to execute.
        // Explicit reborrows let the borrow checker split the struct's &mut fields.
        {
            let sm = &mut *self.snapshot_manager;
            let em = &mut *self.executor_manager;
            let c = &*self.cache;
            let pr = self.project_root;
            sm.restore_if_needed(&req.chunk.language, req.previous_hash, em, c, pr)?;
        }

        let graphics_opts = GraphicsOptions {
            width: req.resolved_options.fig_width,
            height: req.resolved_options.fig_height,
            dpi: req.resolved_options.dpi,
            format: req.resolved_options.fig_format.as_str().to_string(),
        };

        // Scope exec so its borrow on executor_manager ends before we touch cache.
        let output = {
            let exec = self.executor_manager.get_executor(&req.chunk.language)?;
            exec.execute(&req.chunk.code, &graphics_opts)?
        };

        if req.resolved_options.cache {
            if let Some(error) = &output.error {
                self.cache.save_error(
                    req.chunk.index,
                    req.chunk.name.clone(),
                    req.chunk.language.clone(),
                    req.chunk_hash.to_string(),
                    error.clone(),
                    req.chunk_options.depends.clone(),
                )?;
            } else {
                self.cache.save_result(
                    req.chunk.index,
                    req.chunk.name.clone(),
                    req.chunk.language.clone(),
                    req.chunk_hash.to_string(),
                    &output,
                    req.chunk_options.depends.clone(),
                )?;
            }
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Free functions (pure computations, no mutable resource access)
// ---------------------------------------------------------------------------

/// Applies config layering (global → language → error) to produce resolved options and merged codly options.
fn resolve_options(
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

/// Computes the chunk hash, incorporating deps and constant-object hashes.
fn compute_hash(
    code: &str,
    chunk_options: &ChunkOptions,
    previous_hash: &str,
    executor_manager: &mut ExecutorManager,
    language: &str,
) -> Result<String> {
    let deps_hash = hash_dependencies(&chunk_options.depends)?;
    let partial = hashing::get_chunk_hash(code, chunk_options, previous_hash, &deps_hash, "");

    if chunk_options.constant.is_empty() {
        return Ok(partial);
    }

    let constants_hash = get_constants_hash(executor_manager, language, &chunk_options.constant)?;
    Ok(hashing::get_chunk_hash(
        code,
        chunk_options,
        previous_hash,
        &deps_hash,
        &constants_hash,
    ))
}

/// Clones the chunk with merged codly options and delegates to the backend formatter.
fn format_output(
    backend: &TypstBackend,
    chunk: &Chunk,
    merged_codly_options: &HashMap<String, String>,
    resolved_options: &ResolvedChunkOptions,
    output: &ExecutionOutput,
    state: &ChunkExecutionState,
) -> String {
    let mut chunk_with_codly = chunk.clone();
    chunk_with_codly.codly_options = merged_codly_options.clone();
    backend.format_chunk(&chunk_with_codly, resolved_options, output, state)
}

fn get_constants_hash(
    executor_manager: &mut ExecutorManager,
    lang: &str,
    constants: &[String],
) -> Result<String> {
    if constants.is_empty() {
        return Ok(String::new());
    }

    // Fetch all hashes in a single round-trip instead of N separate queries
    let exec = executor_manager.get_executor(lang)?;
    let hashes = exec.hash_objects(constants)?;

    let mut combined_hash = Sha256::new();
    for var in constants {
        match hashes.get(var) {
            Some(hash) if hash != "NONE" => {
                combined_hash.update(var.as_bytes());
                combined_hash.update(hash.as_bytes());
            }
            _ => {
                // Object not found or invalid: use a stable marker to avoid invalidating
                // the cache with random values (like UUIDs).
                combined_hash.update(var.as_bytes());
                combined_hash.update(b"NOT_FOUND");
            }
        }
    }
    Ok(format!("{:x}", combined_hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup_test_cache, setup_test_manager};
    use super::*;
    use crate::parser::{ChunkOptions, Position, Range};
    use tempfile::TempDir;

    fn create_test_chunk(
        language: &str,
        code: &str,
        name: Option<String>,
        eval: bool,
        cache: bool,
    ) -> Chunk {
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        Chunk {
            index: 0, // Test helper: use dummy index
            language: language.to_string(),
            code: code.to_string(),
            name,
            base_indentation: String::new(),
            options: ChunkOptions {
                eval: Some(eval),
                show: Some(crate::parser::Show::Both),
                cache: Some(cache),
                caption: None,
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
            start_byte: 100,
            end_byte: 200,
            code_start_byte: 110,
            code_end_byte: 190,
        }
    }

    fn setup_test_config() -> crate::config::Config {
        crate::config::Config::default()
    }

    /// Convenience helper: build a `ChunkProcessor` rooted at `.` for tests.
    fn make_processor<'a>(
        manager: &'a mut ExecutorManager,
        cache: &'a mut Cache,
        sm: &'a mut SnapshotManager,
        config: &'a crate::config::Config,
        backend: &'a TypstBackend,
    ) -> ChunkProcessor<'a> {
        ChunkProcessor::new(manager, cache, sm, config, backend, Path::new("."))
    }

    #[test]
    fn test_process_chunk_eval_false() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // With eval=false, should produce output but not execute
        assert!(output.contains("#code-chunk("));
        assert!(output.contains("lang: \"r\""));
    }

    #[test]
    fn test_process_chunk_generates_name() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        // Should not panic
        make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();
    }

    #[test]
    fn test_process_chunk_with_name() {
        let chunk = create_test_chunk("r", "x <- 1", Some("my-chunk".to_string()), false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // With name, should include it in output
        assert!(output.contains("my-chunk"));
        assert!(output.contains("#figure("));
    }

    #[test]
    fn test_process_chunk_hash_consistency() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let mut sm1 = SnapshotManager::new();
        let (_output1, hash1) =
            make_processor(&mut manager, &mut cache, &mut sm1, &config, &backend)
                .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
                .unwrap();

        // Process same chunk again with same previous_hash
        let mut sm2 = SnapshotManager::new();
        let (_output2, hash2) =
            make_processor(&mut manager, &mut cache, &mut sm2, &config, &backend)
                .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
                .unwrap();

        // Should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_code() {
        let chunk1 = create_test_chunk("r", "x <- 1", None, false, false);
        let chunk2 = create_test_chunk("r", "x <- 2", None, false, false);

        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let mut sm1 = SnapshotManager::new();
        let (_output1, hash1) =
            make_processor(&mut manager, &mut cache, &mut sm1, &config, &backend)
                .process_chunk(&chunk1, ChunkExecutionState::Ready, "prev_hash")
                .unwrap();

        let mut sm2 = SnapshotManager::new();
        let (_output2, hash2) =
            make_processor(&mut manager, &mut cache, &mut sm2, &config, &backend)
                .process_chunk(&chunk2, ChunkExecutionState::Ready, "prev_hash")
                .unwrap();

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_previous() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let mut sm1 = SnapshotManager::new();
        let (_output1, hash1) =
            make_processor(&mut manager, &mut cache, &mut sm1, &config, &backend)
                .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash_1")
                .unwrap();

        let mut sm2 = SnapshotManager::new();
        let (_output2, hash2) =
            make_processor(&mut manager, &mut cache, &mut sm2, &config, &backend)
                .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash_2")
                .unwrap();

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_unsupported_language() {
        let chunk = create_test_chunk("unsupported_lang", "print(42)", None, true, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let result = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash");

        // Should fail with unsupported language
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported language")
        );
    }

    #[test]
    fn test_process_chunk_with_dependencies() {
        use std::fs;

        let mut chunk = create_test_chunk("r", "y <- x * 2", None, false, true);

        // Create temporary dependency files
        let temp_dir = TempDir::new().unwrap();
        let dep1 = temp_dir.path().join("dep1.txt");
        let dep2 = temp_dir.path().join("dep2.txt");
        let dep3 = temp_dir.path().join("dep3.txt");

        fs::write(&dep1, "content1").unwrap();
        fs::write(&dep2, "content2").unwrap();
        fs::write(&dep3, "content3").unwrap();

        chunk.options.depends = vec![dep1.clone(), dep2.clone()];

        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let mut sm1 = SnapshotManager::new();
        let (_output, hash) = make_processor(&mut manager, &mut cache, &mut sm1, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Hash should incorporate dependencies
        assert!(!hash.is_empty());

        // Changing dependencies should change hash
        chunk.options.depends = vec![dep1.clone(), dep3.clone()];
        let mut sm2 = SnapshotManager::new();
        let (_output2, hash2) =
            make_processor(&mut manager, &mut cache, &mut sm2, &config, &backend)
                .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
                .unwrap();

        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_process_chunk_empty_code() {
        let chunk = create_test_chunk("r", "", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Should handle empty code gracefully
        assert!(output.contains("#code-chunk("));
    }

    #[test]
    fn test_process_chunk_output_contains_language() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();
        let mut sm = SnapshotManager::new();
        let config = setup_test_config();
        let backend = crate::backend::TypstBackend::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Output should indicate language
        assert!(output.contains("lang: \"r\""));
    }

    #[test]
    fn test_process_chunk_language_specific_defaults() {
        use crate::config::{ChunkDefaults, Config};
        use crate::parser::{ChunkOptions, Position, Range};

        // Create a chunk with minimal explicit options (show is None)
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        let chunk = Chunk {
            index: 0,
            language: "r".to_string(),
            code: "x <- 1".to_string(),
            name: None,
            base_indentation: String::new(),
            options: ChunkOptions {
                eval: None,      // Will use language default
                show: None,      // Will use language default
                cache: None,     // Will use language default
                fig_width: None, // Will use language default
                ..Default::default()
            },
            codly_options: std::collections::HashMap::new(),
            errors: vec![],
            range: dummy_range.clone(),
            code_range: dummy_range,
            start_byte: 100,
            end_byte: 200,
            code_start_byte: 110,
            code_end_byte: 190,
        };

        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        // Create config with language-specific defaults for R
        let config = Config {
            r_chunks: Some(ChunkDefaults {
                show: Some(crate::parser::Show::Output),
                eval: Some(false),
                cache: Some(true),
                fig_width: Some(10.0),
                fig_height: Some(8.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let backend = crate::backend::TypstBackend::new();
        let mut sm = SnapshotManager::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Verify that language-specific defaults were applied (show: output means code: none)
        assert!(output.contains("code: none"));
    }

    #[test]
    fn test_process_chunk_language_defaults_priority() {
        use crate::config::{ChunkDefaults, Config};

        // Create a chunk with some explicit options
        let mut chunk = create_test_chunk("python", "x = 1", None, false, false);
        chunk.options.show = Some(crate::parser::Show::Both); // Override with chunk-specific option

        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        // Create config with both global and language-specific defaults
        let config = Config {
            chunk_defaults: ChunkDefaults {
                show: Some(crate::parser::Show::Output), // Global default
                ..Default::default()
            },
            python_chunks: Some(ChunkDefaults {
                show: Some(crate::parser::Show::Output), // Language-specific default
                fig_width: Some(6.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let backend = crate::backend::TypstBackend::new();
        let mut sm = SnapshotManager::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Chunk-specific option should override everything (show: both means code is shown)
        assert!(output.contains("code: [```python"));
    }

    #[test]
    fn test_process_chunk_global_defaults_fallback() {
        use crate::config::{ChunkDefaults, Config};
        use crate::parser::{ChunkOptions, Position, Range};

        // Create a Python chunk (supported language) without language-specific defaults
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        let chunk = Chunk {
            index: 0,
            language: "python".to_string(),
            code: "x = 1".to_string(),
            name: None,
            base_indentation: String::new(),
            options: ChunkOptions {
                eval: None, // Will use global default
                show: None, // Will use global default
                ..Default::default()
            },
            codly_options: std::collections::HashMap::new(),
            errors: vec![],
            range: dummy_range.clone(),
            code_range: dummy_range,
            start_byte: 100,
            end_byte: 200,
            code_start_byte: 110,
            code_end_byte: 190,
        };

        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        // Create config with only global defaults (no python-chunks defined)
        let config = Config {
            chunk_defaults: ChunkDefaults {
                show: Some(crate::parser::Show::Output),
                eval: Some(false),
                ..Default::default()
            },
            // Note: python_chunks is None, so global defaults should be used
            ..Default::default()
        };
        let backend = crate::backend::TypstBackend::new();
        let mut sm = SnapshotManager::new();

        let (output, _hash) = make_processor(&mut manager, &mut cache, &mut sm, &config, &backend)
            .process_chunk(&chunk, ChunkExecutionState::Ready, "prev_hash")
            .unwrap();

        // Should use global defaults (show: output means code is not shown)
        assert!(output.contains("code: none"));
    }
}
