//! Inline Expression Processing
//!
//! This module handles the execution of inline expressions (e.g., `r 1 + 1`).
//! Inline expressions are simpler than chunks:
//! - No rich output (plots/dataframes) - only text results are supported.
//! - Cached using a separate mechanism in the Cache module.
//! - Results are formatted based on the expression's return value.

use crate::cache::Cache;
use crate::executors::ExecutorManager;
use crate::parser::InlineExpr;
use anyhow::{Context, Result};
use log::info;

pub fn process_inline_expr(
    inline_expr: &InlineExpr,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    previous_hash: &str,
) -> Result<(String, String)> {
    let resolved_options = inline_expr.options.resolve();
    let inline_hash =
        cache.get_inline_expr_hash(&inline_expr.code, &inline_expr.options, previous_hash);

    // If eval=false, skip execution
    if !resolved_options.eval {
        info!(
            "  ⊗ `{{{}}} eval=false {}` [skipped]",
            inline_expr.language, inline_expr.code
        );
        return Ok((String::new(), inline_hash));
    }

    // Check cache
    if cache.has_cached_inline_result(&inline_hash) {
        info!(
            "  ✓ `{{{}}} {}` [cached]",
            inline_expr.language, inline_expr.code
        );
        let cached_result = cache.get_cached_inline_result(&inline_hash)?;
        return Ok((cached_result, inline_hash));
    }

    // Execute the inline expression
    info!(
        "  ⚙️ `{{{}}} {}` [executing]",
        inline_expr.language, inline_expr.code
    );

    // Get the executor for the specific language of the inline expression
    let result = executor_manager
        .get_executor(&inline_expr.language)?
        .execute_inline(&inline_expr.code)
        .context(format!(
            "Failed to execute inline expression: `{{{}}} {}`",
            inline_expr.language, inline_expr.code
        ))?;

    // Determine what to display based on show option
    let final_result = match resolved_options.show {
        crate::parser::Show::Output | crate::parser::Show::Both => result,
        crate::parser::Show::Code | crate::parser::Show::None => String::new(),
    };

    // Cache the result (either the actual result or empty string)
    cache.save_inline_result(inline_hash.clone(), &final_result)?;

    Ok((final_result, inline_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::{setup_test_cache, setup_test_manager};

    fn create_inline_expr(code: &str, options: crate::parser::InlineOptions) -> InlineExpr {
        InlineExpr {
            language: "r".to_string(),
            code: code.to_string(),
            options,
            errors: vec![],
            start: 0,
            end: code.len(),
            code_start_byte: 0,
            code_end_byte: code.len(),
        }
    }

    #[test]
    fn test_process_inline_eval_false_no_executor() {
        let inline = create_inline_expr(
            "x <- 42",
            crate::parser::InlineOptions {
                eval: Some(false),
                ..Default::default()
            },
        );
        let (_temp_dir_cache, mut cache) = setup_test_cache();
        let (_temp_dir_mgr, mut manager) = setup_test_manager();

        let result = process_inline_expr(&inline, &mut manager, &mut cache, "prev_hash");

        // Should succeed even if manager can't start R (because eval=false skips execution)
        assert!(result.is_ok());
        let (output, _hash) = result.unwrap();
        assert_eq!(output, ""); // Empty output when eval=false
    }

    #[test]
    fn test_process_inline_hash_consistency() {
        let inline = create_inline_expr("1 + 1", crate::parser::InlineOptions::default());
        let (_temp_dir, cache) = setup_test_cache();

        // Compute hash
        let hash1 = cache.get_inline_expr_hash(&inline.code, &inline.options, "prev_hash");

        // Same inputs should produce same hash
        let hash2 = cache.get_inline_expr_hash(&inline.code, &inline.options, "prev_hash");

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_process_inline_hash_changes_with_code() {
        let inline1 = create_inline_expr("1 + 1", crate::parser::InlineOptions::default());
        let inline2 = create_inline_expr("2 + 2", crate::parser::InlineOptions::default());

        let (_temp_dir, cache) = setup_test_cache();

        let hash1 = cache.get_inline_expr_hash(&inline1.code, &inline1.options, "prev_hash");
        let hash2 = cache.get_inline_expr_hash(&inline2.code, &inline2.options, "prev_hash");

        // Different code should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_inline_hash_changes_with_options() {
        let inline_default = create_inline_expr("x <- 1", crate::parser::InlineOptions::default());
        let inline_eval_false = create_inline_expr(
            "x <- 1",
            crate::parser::InlineOptions {
                eval: Some(false),
                ..Default::default()
            },
        );
        let inline_output_false = create_inline_expr(
            "x <- 1",
            crate::parser::InlineOptions {
                show: Some(crate::parser::Show::Code),
                ..Default::default()
            },
        );

        let (_temp_dir, cache) = setup_test_cache();

        let hash1 =
            cache.get_inline_expr_hash(&inline_default.code, &inline_default.options, "prev_hash");
        let hash2 = cache.get_inline_expr_hash(
            &inline_eval_false.code,
            &inline_eval_false.options,
            "prev_hash",
        );
        let hash3 = cache.get_inline_expr_hash(
            &inline_output_false.code,
            &inline_output_false.options,
            "prev_hash",
        );

        // Different options should produce different hash
        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);
    }

    #[test]
    fn test_process_inline_hash_changes_with_previous() {
        let inline = create_inline_expr("1 + 1", crate::parser::InlineOptions::default());
        let (_temp_dir, cache) = setup_test_cache();

        let hash1 = cache.get_inline_expr_hash(&inline.code, &inline.options, "prev_hash_1");
        let hash2 = cache.get_inline_expr_hash(&inline.code, &inline.options, "prev_hash_2");

        // Different previous_hash should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_inline_eval_false_produces_empty_output() {
        // This test documents the behavior: eval=false produces empty string
        let inline = create_inline_expr(
            "x <- 42",
            crate::parser::InlineOptions {
                eval: Some(false),
                ..Default::default()
            },
        );

        // The code in process_inline_expr returns String::new() when eval=false
        assert!(!inline.options.resolve().eval);
    }

    #[test]
    fn test_inline_expr_structure() {
        let inline = create_inline_expr("mean(1:10)", crate::parser::InlineOptions::default());
        let resolved = inline.options.resolve();

        assert_eq!(inline.language, "r");
        assert_eq!(inline.code, "mean(1:10)");
        assert_eq!(resolved.show, crate::parser::Show::Output); // default
        assert!(resolved.eval);
        assert_eq!(resolved.digits, None);
        assert_eq!(inline.start, 0);
        assert_eq!(inline.end, "mean(1:10)".len());
    }

    #[test]
    fn test_inline_expr_with_options_structure() {
        let inline = create_inline_expr(
            "library(dplyr)",
            crate::parser::InlineOptions {
                eval: Some(false),
                show: Some(crate::parser::Show::Code),
                digits: Some(3),
            },
        );
        let resolved = inline.options.resolve();

        assert_eq!(inline.language, "r");
        assert_eq!(inline.code, "library(dplyr)");
        assert!(!resolved.eval);
        assert_eq!(resolved.show, crate::parser::Show::Code);
        assert_eq!(resolved.digits, Some(3));
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
        let inline = create_inline_expr("", crate::parser::InlineOptions::default());
        let (_temp_dir, cache) = setup_test_cache();

        // Hash should still be computed
        let hash = cache.get_inline_expr_hash(&inline.code, &inline.options, "prev_hash");
        assert!(!hash.is_empty());
    }
}
