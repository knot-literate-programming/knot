pub mod parser;
pub mod executors;
pub mod compiler;
pub mod backend;
pub mod cache;
pub mod graphics;

pub use parser::{Chunk, ChunkOptions, Document, InlineExpr};
pub use compiler::Compiler;
pub use graphics::{GraphicsDefaults, GraphicsConfig, ResolvedGraphicsOptions, resolve_graphics_options};

use std::path::PathBuf;
use anyhow::Result;

/// Returns the path to the knot cache directory.
/// By default, this is `.knot_cache` in the current working directory.
///
/// This centralizes the cache directory configuration to avoid inconsistencies.
pub fn get_cache_dir() -> PathBuf {
    PathBuf::from(".knot_cache")
}

