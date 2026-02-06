pub mod backend;
pub mod cache;
pub mod compiler;
pub mod config;
pub mod defaults;
pub mod executors;
pub mod graphics;
pub mod parser;

pub use compiler::Compiler;
pub use config::{ChunkDefaults, Config};
pub use defaults::Defaults;
pub use graphics::{GraphicsDefaults, ResolvedGraphicsOptions, resolve_graphics_options};
pub use parser::{Chunk, ChunkOptions, Document, InlineExpr, ResolvedChunkOptions};

pub const R_HELPER_SCRIPT: &str = include_str!("../resources/typst.R");
pub const PYTHON_HELPER_SCRIPT: &str = include_str!("../resources/typst.py");

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the knot cache directory.
///
/// # Arguments
/// * `project_root` - Path to the project root directory
/// * `sub_dir` - Sub-directory for isolation (e.g., "main" or "01-intro")
pub fn get_cache_dir(project_root: &Path, sub_dir: &str) -> PathBuf {
    project_root.join(Defaults::CACHE_DIR_NAME).join(sub_dir)
}

/// Clean project (remove cache and generated files)
///
/// # Arguments
/// * `start_dir` - Optional directory to start searching for knot.toml.
///   If None, uses current working directory.
pub fn clean_project(start_dir: Option<&Path>) -> Result<()> {
    use log::info;

    info!("🧹 Cleaning project...");

    // 1. Find project root
    let search_dir = if let Some(dir) = start_dir {
        dir.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let (_, project_root) = config::Config::find_and_load(&search_dir)?;

    info!("📁 Project root: {}", project_root.display());

    // 2. Remove .knot_cache directory
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .with_context(|| format!("Failed to remove cache directory: {:?}", cache_dir))?;
        println!(
            "  ✓ Removed cache directory: {:?}",
            Defaults::CACHE_DIR_NAME
        );
    }

    // 3. Remove _knot_r_files directory
    let r_files_dir = project_root.join(Defaults::R_FILES_DIR);
    if r_files_dir.exists() {
        fs::remove_dir_all(&r_files_dir).with_context(|| {
            format!("Failed to remove helper files directory: {:?}", r_files_dir)
        })?;
        println!(
            "  ✓ Removed helper files directory: {:?}",
            Defaults::R_FILES_DIR
        );
    }

    // 4. Remove hidden .*.typ and .*.pdf files in the project root
    let entries = fs::read_dir(&project_root)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if path.is_file()
                && filename.starts_with('.')
                && (filename.ends_with(".typ") || filename.ends_with(".pdf"))
            {
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove file: {:?}", path))?;
                println!("  ✓ Removed generated file: {}", filename);
            }
        }
    }

    Ok(())
}
