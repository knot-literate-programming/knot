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
                eval,
                echo: true,
                output: true,
                cache,
                caption: None,
                depends: vec![],
                label: None,
                fig_width: None,
                fig_height: None,
                dpi: None,
                fig_format: None,
                fig_alt: None,
            },
            range: dummy_range.clone(),
            code_range: dummy_range,
            start_byte: 100,
            end_byte: 200,
        }
    }

    fn setup_test_cache() -> (TempDir, Cache) {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, cache)
    }

    fn setup_test_r_executor() -> Option<RExecutor> {
        // For most tests, we'll use None to avoid needing R installed
        // Some tests will explicitly set up an executor if needed
        None
    }

    #[test]
    fn test_process_chunk_eval_false() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // With eval=false, should produce output but not execute
        assert!(output.contains("#code-chunk("));
        assert!(output.contains("lang: \"r\""));
        // Should not have executed (no R executor needed)
    }

    #[test]
    fn test_process_chunk_generates_name() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (_output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Should generate name from start_byte (100)
        // We can't directly check the name, but the function should not panic
    }

    #[test]
    fn test_process_chunk_with_name() {
        let chunk = create_test_chunk("r", "x <- 1", Some("my-chunk".to_string()), false, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // With name, should include it in output
        assert!(output.contains("my-chunk"));
        assert!(output.contains("#figure("));
    }

    #[test]
    fn test_process_chunk_hash_consistency() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (_output1, hash1) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Process same chunk again with same previous_hash
        let (_output2, hash2) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_code() {
        let chunk1 = create_test_chunk("r", "x <- 1", None, false, false);
        let chunk2 = create_test_chunk("r", "x <- 2", None, false, false);

        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (_output1, hash1) = process_chunk(&chunk1, &mut executor, &mut cache, "prev_hash")
            .unwrap();
        let (_output2, hash2) = process_chunk(&chunk2, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_hash_changes_with_previous() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (_output1, hash1) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash_1")
            .unwrap();
        let (_output2, hash2) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash_2")
            .unwrap();

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_chunk_unsupported_language() {
        let chunk = create_test_chunk("python", "print(42)", None, true, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Should handle unsupported language gracefully
        assert!(output.contains("python"));
        assert!(output.contains("not supported"));
    }

    #[test]
    fn test_process_chunk_caching_disabled() {
        let mut chunk = create_test_chunk("r", "x <- 1", None, true, false);
        chunk.options.cache = false;

        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        // First call (should not cache because R executor not available)
        let result1 = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash");

        // Should fail because we need R executor when eval=true and language=r
        assert!(result1.is_err());
        assert!(result1.unwrap_err().to_string().contains("R executor not initialized"));
    }

    #[test]
    fn test_process_chunk_no_executor_with_eval_true() {
        let chunk = create_test_chunk("r", "x <- 1", None, true, true);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = None; // No executor

        let result = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash");

        // Should error when trying to execute without executor
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("R executor not initialized"));
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

        let (_cache_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (_output, hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Hash should incorporate dependencies
        assert!(!hash.is_empty());

        // Changing dependencies should change hash
        chunk.options.depends = vec![dep1.clone(), dep3.clone()];
        let (_output2, hash2) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        assert_ne!(hash, hash2);
    }

    #[test]
    fn test_process_chunk_empty_code() {
        let chunk = create_test_chunk("r", "", None, false, false);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Should handle empty code gracefully
        assert!(output.contains("#code-chunk("));
    }

    #[test]
    fn test_process_chunk_output_contains_language() {
        let chunk = create_test_chunk("r", "x <- 1", None, false, true);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor = setup_test_r_executor();

        let (output, _hash) = process_chunk(&chunk, &mut executor, &mut cache, "prev_hash")
            .unwrap();

        // Output should indicate language
        assert!(output.contains("lang: \"r\""));
    }
}
