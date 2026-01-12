pub mod parser;
pub mod executors;
pub mod compiler;
pub mod codegen;
pub mod cache;
pub mod graphics;

pub use parser::{Chunk, ChunkOptions, Document, InlineExpr};
pub use compiler::Compiler;
pub use graphics::{GraphicsDefaults, GraphicsConfig, ResolvedGraphicsOptions, resolve_graphics_options};

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;
use anyhow::Result;

/// Shared regex pattern for matching code chunks in .knot documents.
/// This pattern is used by both the parser and code generator to ensure consistency.
///
/// Pattern groups:
/// - `lang`: The programming language (r, python, lilypond)
/// - `name`: Optional chunk name
/// - `options`: Block of #| option lines
/// - `code`: The actual code content
pub static CHUNK_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?s)```\{(?P<lang>r|python|lilypond)\s*(?P<name>[^}]*)\}\n(?P<options>(?:#\|[^\n]*\n)*)(?P<code>.*?)```"#
    ).expect("Failed to compile CHUNK_REGEX")
});

/// Returns the path to the knot cache directory.
/// By default, this is `.knot_cache` in the current working directory.
///
/// This centralizes the cache directory configuration to avoid inconsistencies.
pub fn get_cache_dir() -> PathBuf {
    PathBuf::from(".knot_cache")
}

/// Shared regex pattern for detecting inline expression starts: #r[, #python[, etc.
/// Used by both parser and compiler to ensure consistency.
static INLINE_EXPR_START: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"#(r|python|lilypond)\[")
        .expect("Failed to compile INLINE_EXPR_START regex")
});

/// Finds all inline expressions in the text with proper bracket matching.
///
/// This function is shared between the parser and compiler to avoid code duplication.
/// It correctly handles:
/// - Nested brackets (e.g., `#r[letters[1:3]]`)
/// - Escaped expressions (e.g., `\#r[x]` is ignored)
///
/// Returns a vector of `(language, code, start_pos, end_pos)` tuples.
pub fn find_inline_expressions(text: &str) -> Result<Vec<(String, String, usize, usize)>> {
    let mut results = Vec::new();

    for cap in INLINE_EXPR_START.captures_iter(text) {
        let match_start = cap.get(0).unwrap().start();

        // Skip if the # is escaped with a backslash
        if match_start > 0 && text.as_bytes()[match_start - 1] == b'\\' {
            continue;
        }

        let language = cap.get(1).unwrap().as_str().to_string();
        let code_start = cap.get(0).unwrap().end(); // Position after #r[

        // Find the matching closing bracket, handling nesting
        let mut depth = 1;
        let mut code_end = code_start;

        for (i, ch) in text[code_start..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        code_end = code_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if depth != 0 {
            anyhow::bail!(
                "Unmatched bracket in inline expression starting at position {}",
                match_start
            );
        }

        let code = text[code_start..code_end].to_string();
        let match_end = code_end + 1; // +1 for the closing ]

        results.push((language, code, match_start, match_end));
    }

    Ok(results)
}
