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
