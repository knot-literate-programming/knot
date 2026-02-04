pub mod parser;
pub mod executors;
pub mod compiler;
pub mod backend;
pub mod cache;
pub mod graphics;
pub mod config;
pub mod defaults;

pub use parser::{Chunk, ChunkOptions, ResolvedChunkOptions, Document, InlineExpr};
pub use compiler::Compiler;
pub use graphics::{GraphicsDefaults, ResolvedGraphicsOptions, resolve_graphics_options};
pub use config::{Config, ChunkDefaults};
pub use defaults::Defaults;

use std::path::{Path, PathBuf};

/// Returns the path to the knot cache directory.
///
/// # Arguments
/// * `project_root` - Path to the project root directory
/// * `sub_dir` - Sub-directory for isolation (e.g., "main" or "01-intro")
pub fn get_cache_dir(project_root: &Path, sub_dir: &str) -> PathBuf {
    project_root.join(Defaults::CACHE_DIR_NAME).join(sub_dir)
}

