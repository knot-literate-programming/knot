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
use crate::cache::{hash_dependencies, Cache};
use crate::config::Config;
use crate::executors::{ExecutionOutput, ExecutionResult, ExecutorManager, GraphicsOptions};
use crate::parser::Chunk;
use anyhow::Result;
use log::info;
use sha2::{Digest, Sha256};

pub fn process_chunk(
    chunk: &Chunk,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    previous_hash: &str,
    config: &Config,
    is_inert: bool,
) -> Result<(String, String)> {
    // Apply config defaults to chunk options and resolve to concrete values
    let mut chunk_options = chunk.options.clone();

    // --- CONFIG LAYERING (Global < Language < Error) ---
    // We build a single "effective" set of defaults following the priority chain.
    let mut effective_defaults = config.chunk_defaults.clone();

    // 1. Layer language-specific defaults ([r-chunks], [python-chunks])
    if let Some(lang_defaults) = config.get_language_defaults(&chunk.language) {
        effective_defaults.merge(lang_defaults);
    }

    // 2. Layer error-specific defaults ([r-error], [python-error]) if language is broken
    if is_inert && let Some(error_defaults) = config.get_language_error_defaults(&chunk.language) {
        effective_defaults.merge(error_defaults);
    }

    // 3. Apply the final layered defaults to fill Nones in the chunk's own options.
    // Explicit options in the chunk header always have the final word.
    chunk_options.apply_config_defaults(&effective_defaults);

    // Codly options follow the same priority (already merged in effective_defaults)
    let mut merged_codly_options = effective_defaults.codly_options.clone();
    for (key, value) in &chunk.codly_options {
        merged_codly_options.insert(key.clone(), value.clone());
    }

    let resolved_options = chunk_options.resolve(); // Convert Option<bool> to bool
    let chunk_name = chunk
        .name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| format!("chunk-{}", chunk.start_byte));

    let deps_hash = hash_dependencies(&chunk_options.depends)?;
    let constants_hash =
        get_constants_hash(executor_manager, &chunk.language, &chunk_options.constant)?;

    let chunk_hash = cache.get_chunk_hash(
        &chunk.code,
        &chunk_options,
        previous_hash,
        &deps_hash,
        &constants_hash,
    );

    let execution_output = if is_inert || !resolved_options.eval {
        ExecutionOutput {
            result: ExecutionResult::Text(String::new()),
            warnings: vec![],
            error: None,
        }
    } else if resolved_options.cache && cache.has_cached_result(&chunk_hash) {
        info!("  ✓ {} [cached]", chunk_name);
        cache.get_cached_result(&chunk_hash)?
    } else {
        info!("  ⚙️ {} [executing]", chunk_name);

        // Prepare graphics options for executor
        let graphics_opts = GraphicsOptions {
            width: resolved_options.fig_width,
            height: resolved_options.fig_height,
            dpi: resolved_options.dpi,
            format: resolved_options.fig_format.as_str().to_string(),
        };

        let exec = executor_manager.get_executor(&chunk.language)?;
        let output = exec.execute(&chunk.code, &graphics_opts)?;

        if resolved_options.cache {
            if let Some(error) = &output.error {
                // Save execution error to cache
                cache.save_error(
                    chunk.start_byte,
                    chunk.name.clone(),
                    chunk.language.clone(),
                    chunk_hash.clone(),
                    error.clone(),
                    chunk_options.depends.clone(),
                )?;
            } else {
                // Save successful result to cache
                cache.save_result(
                    chunk.start_byte, // Use start_byte as unique ID for now
                    chunk.name.clone(),
                    chunk.language.clone(),
                    chunk_hash.clone(),
                    &output,
                    chunk_options.depends.clone(),
                )?;
            }
        }
        output
    };

    // If there is a fatal error, we need to return it as an Err so the compiler knows to switch to inert mode
    // (preserving the behavior expected by mod.rs)
    if let Some(error) = &execution_output.error {
        // Return error with structured data (via anyhow)
        // We'll wrap it in a way that mod.rs can still format it easily
        return Err(anyhow::anyhow!("{}", error));
    }

    // Create a chunk with merged codly options for the backend
    let mut chunk_with_codly = chunk.clone();
    chunk_with_codly.codly_options = merged_codly_options;

    let backend = TypstBackend::new();
    let chunk_output_final = backend.format_chunk(
        &chunk_with_codly,
        &resolved_options,
        &execution_output,
        is_inert,
    );

    Ok((chunk_output_final, chunk_hash))
}

fn get_constants_hash(
    executor_manager: &mut ExecutorManager,
    lang: &str,
    constants: &[String],
) -> Result<String> {
    if constants.is_empty() {
        return Ok(String::new());
    }

    let exec = executor_manager.get_executor(lang)?;
    let mut combined_hash = Sha256::new();
    for var in constants {
        match exec.hash_object(var) {
            Ok(hash) => {
                combined_hash.update(var.as_bytes());
                combined_hash.update(hash.as_bytes());
            }
            Err(e) => {
                // If we can't hash a constant, it implies it doesn't exist or is invalid.
                // We force invalidation by adding a random component.
                log::warn!("Could not hash constant '{}' in {}: {}", var, lang, e);
                combined_hash.update(var.as_bytes());
                combined_hash.update(uuid::Uuid::new_v4().as_bytes());
            }
        }
    }
    Ok(format!("{:x}", combined_hash.finalize()))
}

#[cfg(test)]
mod tests {
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
            language: language.to_string(),
            code: code.to_string(),
            name,
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

    fn setup_test_cache() -> (TempDir, Cache) {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, cache)
    }

    fn setup_test_manager() -> (TempDir, ExecutorManager) {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExecutorManager::new(temp_dir.path().to_path_buf());
        (temp_dir, manager)
    }

    fn setup_test_config() -> crate::config::Config {
        crate::config::Config::default()
    }

    #[test]
    fn test_process_chunk_eval_false() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
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

        let (_output, _hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();
    }

    #[test]
    fn test_process_chunk_with_name() {
        let chunk = create_test_chunk("r", "x <- 1", Some("my-chunk".to_string()), false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
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

        let (_output1, hash1) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Process same chunk again with same previous_hash
        let (_output2, hash2) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
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

        let (_output1, hash1) = process_chunk(
            &chunk1,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();
        let (_output2, hash2) = process_chunk(
            &chunk2,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_previous() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (_output1, hash1) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash_1",
            &setup_test_config(),
            false,
        )
        .unwrap();
        let (_output2, hash2) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash_2",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_unsupported_language() {
        let chunk = create_test_chunk("unsupported_lang", "print(42)", None, true, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let result = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        );

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

        let (_output, hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Hash should incorporate dependencies
        assert!(!hash.is_empty());

        // Changing dependencies should change hash
        chunk.options.depends = vec![dep1.clone(), dep3.clone()];
        let (_output2, hash2) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_process_chunk_empty_code() {
        let chunk = create_test_chunk("r", "", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Should handle empty code gracefully
        assert!(output.contains("#code-chunk("));
    }

    #[test]
    fn test_process_chunk_output_contains_language() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(
            &chunk,
            &mut manager,
            &mut cache,
            "prev_hash",
            &setup_test_config(),
            false,
        )
        .unwrap();

        // Output should indicate language
        assert!(output.contains("lang: \"r\""));
    }

    #[test]
    fn test_process_chunk_language_specific_defaults() {
        use crate::config::{ChunkDefaults, Config};
        use crate::parser::{ChunkOptions, Position, Range};

        // Create a chunk with minimal explicit options (echo is None)
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        let chunk = Chunk {
            language: "r".to_string(),
            code: "x <- 1".to_string(),
            name: None,
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

        let (output, _hash) =
            process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &config, false).unwrap();

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

        let (output, _hash) =
            process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &config, false).unwrap();

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
            language: "python".to_string(),
            code: "x = 1".to_string(),
            name: None,
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

        let (output, _hash) =
            process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &config, false).unwrap();

        // Should use global defaults (show: output means code is not shown)
        assert!(output.contains("code: none"));
    }
}
