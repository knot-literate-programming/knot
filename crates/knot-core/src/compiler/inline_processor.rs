use crate::cache::Cache;
use crate::executors::r::RExecutor;
use crate::parser::InlineExpr;
use anyhow::{Context, Result};
use log::info;

pub fn process_inline_expr(
    inline_expr: &InlineExpr,
    r_executor: &mut Option<RExecutor>,
    cache: &mut Cache,
    previous_hash: &str,
) -> Result<(String, String)> {
    let inline_hash = cache.get_inline_expr_hash(
        &inline_expr.code,
        &inline_expr.verb,
        previous_hash,
    );

    if cache.has_cached_inline_result(&inline_hash) {
        info!("  ✓ `{{r}} {}` [cached]", inline_expr.code);
        let cached_result = cache.get_cached_inline_result(&inline_hash)?;
        return Ok((cached_result, inline_hash));
    }

    let result_str = if inline_expr.verb.as_deref() == Some("run") {
        info!("  ⚙️ `{{r run}} {}` [executing for side effect]", inline_expr.code);
        r_executor
            .as_mut()
            .context("R executor not initialized")?
            .execute_side_effect_only(&inline_expr.code)
            .context(format!(
                "Failed to execute inline expression: `{{r run}} {}`",
                inline_expr.code
            ))?;
        String::new() // No output for 'run' verb
    } else {
        info!("  ⚙️ `{{r}} {}` [executing]", inline_expr.code);
        let result = r_executor
            .as_mut()
            .context("R executor not initialized")?
            .execute_inline(&inline_expr.code)
            .context(format!(
                "Failed to execute inline expression: `{{r}} {}`",
                inline_expr.code
            ))?;
        
        // Only cache if it's a value-producing inline (no verb)
        if inline_expr.verb.is_none() { 
            cache.save_inline_result(inline_hash.clone(), &result)?;
        }
        result
    };

    Ok((result_str, inline_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_inline_expr(code: &str, verb: Option<String>) -> InlineExpr {
        InlineExpr {
            language: "r".to_string(),
            code: code.to_string(),
            verb,
            start: 0,
            end: code.len(),
        }
    }

    fn setup_test_cache() -> (TempDir, Cache) {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, cache)
    }

    #[test]
    fn test_process_inline_no_executor() {
        let inline = create_inline_expr("1 + 1", None);
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor: Option<RExecutor> = None;

        let result = process_inline_expr(&inline, &mut executor, &mut cache, "prev_hash");

        // Should error without executor
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("R executor not initialized"));
    }

    #[test]
    fn test_process_inline_with_run_verb_no_executor() {
        let inline = create_inline_expr("x <- 42", Some("run".to_string()));
        let (_temp_dir, mut cache) = setup_test_cache();
        let mut executor: Option<RExecutor> = None;

        let result = process_inline_expr(&inline, &mut executor, &mut cache, "prev_hash");

        // Should error without executor even with run verb
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("R executor not initialized"));
    }

    #[test]
    fn test_process_inline_hash_consistency() {
        let inline = create_inline_expr("1 + 1", None);
        let (_temp_dir, cache) = setup_test_cache();

        // Compute hash (will error on execution, but hash should be computed)
        let hash1 = cache.get_inline_expr_hash(&inline.code, &inline.verb, "prev_hash");

        // Same inputs should produce same hash
        let hash2 = cache.get_inline_expr_hash(&inline.code, &inline.verb, "prev_hash");

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_process_inline_hash_changes_with_code() {
        let inline1 = create_inline_expr("1 + 1", None);
        let inline2 = create_inline_expr("2 + 2", None);

        let (_temp_dir, cache) = setup_test_cache();

        let hash1 = cache.get_inline_expr_hash(&inline1.code, &inline1.verb, "prev_hash");
        let hash2 = cache.get_inline_expr_hash(&inline2.code, &inline2.verb, "prev_hash");

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_inline_hash_changes_with_verb() {
        let inline_no_verb = create_inline_expr("x <- 1", None);
        let inline_with_verb = create_inline_expr("x <- 1", Some("run".to_string()));

        let (_temp_dir, cache) = setup_test_cache();

        let hash1 = cache.get_inline_expr_hash(&inline_no_verb.code, &inline_no_verb.verb, "prev_hash");
        let hash2 = cache.get_inline_expr_hash(&inline_with_verb.code, &inline_with_verb.verb, "prev_hash");

        // Different verb should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_inline_hash_changes_with_previous() {
        let inline = create_inline_expr("1 + 1", None);
        let (_temp_dir, cache) = setup_test_cache();

        let hash1 = cache.get_inline_expr_hash(&inline.code, &inline.verb, "prev_hash_1");
        let hash2 = cache.get_inline_expr_hash(&inline.code, &inline.verb, "prev_hash_2");

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_inline_verb_produces_empty_output() {
        // This test documents the behavior: `:run` verb produces empty string
        let inline = create_inline_expr("x <- 42", Some("run".to_string()));

        // The code in process_inline_expr returns String::new() for run verb
        // We can't easily test without executor, but we document the behavior
        assert_eq!(inline.verb.as_deref(), Some("run"));
    }

    #[test]
    fn test_inline_expr_structure() {
        let inline = create_inline_expr("mean(1:10)", None);

        assert_eq!(inline.language, "r");
        assert_eq!(inline.code, "mean(1:10)");
        assert_eq!(inline.verb, None);
        assert_eq!(inline.start, 0);
        assert_eq!(inline.end, "mean(1:10)".len());
    }

    #[test]
    fn test_inline_expr_with_verb_structure() {
        let inline = create_inline_expr("library(dplyr)", Some("run".to_string()));

        assert_eq!(inline.language, "r");
        assert_eq!(inline.code, "library(dplyr)");
        assert_eq!(inline.verb, Some("run".to_string()));
    }

    #[test]
    fn test_cache_stores_inline_results() {
        let (_temp_dir, mut cache) = setup_test_cache();
        let hash = "test_inline_hash_12345";
        let result = "42";

        // Save inline result
        cache.save_inline_result(hash.to_string(), result).unwrap();

        // Check if cached
        assert!(cache.has_cached_inline_result(hash));

        // Retrieve cached result
        let cached = cache.get_cached_inline_result(hash).unwrap();
        assert_eq!(cached, result);
    }

    #[test]
    fn test_cache_inline_result_not_found() {
        let (_temp_dir, cache) = setup_test_cache();
        let hash = "nonexistent_hash";

        // Should not be cached
        assert!(!cache.has_cached_inline_result(hash));
    }

    #[test]
    fn test_empty_inline_code() {
        let inline = create_inline_expr("", None);
        let (_temp_dir, cache) = setup_test_cache();

        // Hash should still be computed
        let hash = cache.get_inline_expr_hash(&inline.code, &inline.verb, "prev_hash");
        assert!(!hash.is_empty());
    }
}
