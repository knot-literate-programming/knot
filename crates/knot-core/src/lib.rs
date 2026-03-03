pub mod backend;
pub mod cache;
pub mod compiler;
pub mod config;
pub mod defaults;
pub mod executors;
pub mod graphics;
pub mod parser;
pub mod project;

pub use backend::{format_codly_call, format_local_call};
pub use compiler::Compiler;
pub use compiler::formatters::CodeFormatter;
pub use compiler::sync;
pub use compiler::{
    ExecutedNode, PlannedNode, ProgressEvent, assemble_pass, planned_to_partial_nodes,
};
pub use config::{ChunkDefaults, Config};
pub use defaults::Defaults;
pub use graphics::{GraphicsDefaults, ResolvedGraphicsOptions, resolve_graphics_options};
pub use parser::{Chunk, ChunkOptions, Document, InlineExpr, ResolvedChunkOptions};
pub use project::{
    ProjectOutput, compile_project_full, compile_project_phase0, compile_project_phase0_unsaved,
};

// R helper scripts (loaded in order)
pub const R_HELPERS: &[(&str, &str)] = &[
    ("helpers.R", include_str!("../resources/r/helpers.R")),
    ("executor.R", include_str!("../resources/r/executor.R")),
    ("session.R", include_str!("../resources/r/session.R")),
    ("constants.R", include_str!("../resources/r/constants.R")),
    ("output.R", include_str!("../resources/r/output.R")),
    ("lsp.R", include_str!("../resources/r/lsp.R")),
];

// Python helper scripts (loaded in order)
pub const PYTHON_HELPERS: &[(&str, &str)] = &[
    ("helpers.py", include_str!("../resources/python/helpers.py")),
    (
        "executor.py",
        include_str!("../resources/python/executor.py"),
    ),
    ("session.py", include_str!("../resources/python/session.py")),
    (
        "constants.py",
        include_str!("../resources/python/constants.py"),
    ),
    ("output.py", include_str!("../resources/python/output.py")),
    ("lsp.py", include_str!("../resources/python/lsp.py")),
];

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
/// * `start_path` - Optional path (file or directory) to start searching for knot.toml.
///   If a file is provided, starts searching from its parent directory.
///   If None, uses current working directory.
pub fn clean_project(start_path: Option<&Path>) -> Result<()> {
    use log::info;

    info!("🧹 Cleaning project...");

    // 1. Find project root (handles both files and directories)
    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let project_root = config::Config::find_project_root(&search_path)?;

    info!("📁 Project root: {}", project_root.display());

    // 2. Remove .knot_cache directory
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .with_context(|| format!("Failed to remove cache directory: {:?}", cache_dir))?;
        info!(
            "  ✓ Removed cache directory: {:?}",
            Defaults::CACHE_DIR_NAME
        );
    }

    // 3. Remove _knot_files directory
    let r_files_dir = project_root.join(Defaults::LANGUAGE_FILES_DIR);
    if r_files_dir.exists() {
        fs::remove_dir_all(&r_files_dir).with_context(|| {
            format!("Failed to remove helper files directory: {:?}", r_files_dir)
        })?;
        info!(
            "  ✓ Removed helper files directory: {:?}",
            Defaults::LANGUAGE_FILES_DIR
        );
    }

    // 4. Remove generated .typ and .pdf files
    // Read knot.toml to get the main filename and derive stem
    let (config, _) = config::Config::find_and_load(&project_root)?;
    if let Some(main_file_name) = config.document.main {
        let main_stem = Path::new(&main_file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");

        // Remove {stem}.typ and {stem}.pdf (e.g., main.typ, main.pdf)
        let typ_file = project_root.join(format!("{}.typ", main_stem));
        let pdf_file = project_root.join(format!("{}.pdf", main_stem));

        if typ_file.exists() {
            fs::remove_file(&typ_file)
                .with_context(|| format!("Failed to remove file: {:?}", typ_file))?;
            info!("  ✓ Removed {}.typ", main_stem);
        }

        if pdf_file.exists() {
            fs::remove_file(&pdf_file)
                .with_context(|| format!("Failed to remove file: {:?}", pdf_file))?;
            info!("  ✓ Removed {}.pdf", main_stem);
        }
    }

    // Also remove any hidden .*.typ and .*.pdf files (legacy or intermediate files)
    let entries = fs::read_dir(&project_root)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let filename = path.file_name().and_then(|n| n.to_str());

        match (path.is_file(), filename) {
            (true, Some(name))
                if name.starts_with('.') && (name.ends_with(".typ") || name.ends_with(".pdf")) =>
            {
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove file: {:?}", path))?;
                info!("  ✓ Removed legacy file: {}", name);
            }
            _ => {}
        }
    }

    Ok(())
}
