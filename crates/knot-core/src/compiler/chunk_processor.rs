use crate::executors::{GraphicsOptions, LanguageExecutor, ConstantObjectHandler, ExecutorManager};
use crate::cache::{Cache, hash_dependencies};
use crate::executors::ExecutionResult;
use crate::parser::Chunk;
use crate::config::ChunkDefaults;
use crate::backend::{Backend, TypstBackend};
use anyhow::{Context, Result};
use log::info;
use sha2::{Digest, Sha256};

pub fn process_chunk(
    chunk: &Chunk,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    previous_hash: &str,
    config_defaults: &ChunkDefaults,
) -> Result<(String, String)> {
    // Apply config defaults to chunk options and resolve to concrete values
    let mut chunk_options = chunk.options.clone();
    chunk_options.apply_config_defaults(config_defaults);
    let resolved_options = chunk_options.resolve(); // Convert Option<bool> to bool
    let chunk_name = chunk
        .name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| format!("chunk-{}", chunk.start_byte));

    let deps_hash = hash_dependencies(&chunk_options.depends)?;
    let constants_hash = get_constants_hash(executor_manager, &chunk.language, &chunk_options.constant)?;

    let chunk_hash = cache.get_chunk_hash(
        &chunk.code,
        &chunk_options,
        previous_hash,
        &deps_hash,
        &constants_hash,
    );

    let execution_result = if !resolved_options.eval {
        ExecutionResult::Text(String::new())
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
            format: resolved_options.fig_format.clone(),
        };

        let exec = executor_manager.get_executor(&chunk.language)?;
        let result = exec.execute(&chunk.code, &graphics_opts)?;
        
        if resolved_options.cache {
            cache.save_result(
                chunk.start_byte, // Use start_byte as unique ID for now
                chunk.name.clone(),
                chunk_hash.clone(),
                &result,
                chunk_options.depends.clone(),
            )?;
        }
        result
    };

    let backend = TypstBackend::new();
    let chunk_output_final = backend.format_chunk(chunk, &resolved_options, &execution_result);

    Ok((chunk_output_final, chunk_hash))
}

fn get_constants_hash(
    executor_manager: &mut ExecutorManager,
    lang: &str,
    constants: &[String]
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
            },
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
                echo: Some(true),
                output: Some(true),
                cache: Some(cache),
                caption: None,
                depends: vec![],
                label: None,
                fig_width: None,
                fig_height: None,
                dpi: None,
                fig_format: None,
                fig_alt: None,
                constant: vec![],
            },
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
        let manager = ExecutorManager::new(temp_dir.path().to_path_buf(), None);
        (temp_dir, manager)
    }

    #[test]
    fn test_process_chunk_eval_false() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
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

        let (_output, _hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();
    }

    #[test]
    fn test_process_chunk_with_name() {
        let chunk = create_test_chunk("r", "x <- 1", Some("my-chunk".to_string()), false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
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

        let (_output1, hash1) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        // Process same chunk again with same previous_hash
        let (_output2, hash2) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
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

        let (_output1, hash1) = process_chunk(&chunk1, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();
        let (_output2, hash2) = process_chunk(&chunk2, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_previous() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (_output1, hash1) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash_1", &ChunkDefaults::default())
            .unwrap();
        let (_output2, hash2) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash_2", &ChunkDefaults::default())
            .unwrap();

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_unsupported_language() {
        let chunk = create_test_chunk("python", "print(42)", None, true, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let result = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default());

        // Should fail with unsupported language
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported language"));
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

        let (_output, hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        // Hash should incorporate dependencies
        assert!(!hash.is_empty());

        // Changing dependencies should change hash
        chunk.options.depends = vec![dep1.clone(), dep3.clone()];
        let (_output2, hash2) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_process_chunk_empty_code() {
        let chunk = create_test_chunk("r", "", None, false, false);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        // Should handle empty code gracefully
        assert!(output.contains("#code-chunk("));
    }

    #[test]
    fn test_process_chunk_output_contains_language() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let (output, _hash) = process_chunk(&chunk, &mut manager, &mut cache, "prev_hash", &ChunkDefaults::default())
            .unwrap();

        // Output should indicate language
        assert!(output.contains("lang: \"r\""));
    }
}