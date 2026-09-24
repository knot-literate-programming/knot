use crate::{Config, Defaults, cache::CacheMetadata};
use anyhow::{Context, Result};
use std::{fs, path::Path};

/// Totals for a successfully completed project cleanup.
#[derive(Debug, Default)]
pub struct CleanSummary {
    /// Cached chunk entries removed, excluding inline expressions.
    pub chunks: usize,
    /// Cache indexes that could not be read; the chunk total is incomplete.
    pub unreadable_indexes: usize,
    /// Files and symlinks removed (directory contents included).
    pub files: usize,
}

impl CleanSummary {
    // Inspect links themselves: neither count nor traverse their targets.
    fn inspect(&mut self, path: &Path, cache: bool) -> Result<()> {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("Failed to inspect {}", path.display()))?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                self.inspect(&entry?.path(), cache)?;
            }
        } else {
            self.files += 1;
            if cache && path.file_name().is_some_and(|name| name == "metadata.json") {
                let index = if metadata.is_file() {
                    fs::read(path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<CacheMetadata>(&bytes).ok())
                } else {
                    None
                };
                if let Some(index) = index {
                    self.chunks += index.chunks.len();
                } else {
                    self.unreadable_indexes += 1;
                }
            }
        }
        Ok(())
    }
}

/// Clean project (remove cache and generated files)
///
/// # Arguments
/// * `start_path` - Optional path (file or directory) to start searching for knot.toml.
///   If a file is provided, starts searching from its parent directory.
///   If None, uses current working directory.
pub fn clean_project(start_path: Option<&Path>) -> Result<()> {
    clean_project_with_summary(start_path).map(|_| ())
}

/// Clean a project and report the cache entries and files removed.
pub fn clean_project_with_summary(start_path: Option<&Path>) -> Result<CleanSummary> {
    use log::info;
    let mut summary = CleanSummary::default();

    info!("🧹 Cleaning project...");

    // 1. Find project root (handles both files and directories)
    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let project_root = Config::find_project_root(&search_path)?;

    info!("📁 Project root: {}", project_root.display());

    let (config, _) = Config::find_and_load(&project_root)?;

    // 2. Remove .knot_cache directory
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);
    if cache_dir.exists() {
        summary.inspect(&cache_dir, true)?;
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
        summary.inspect(&r_files_dir, false)?;
        fs::remove_dir_all(&r_files_dir).with_context(|| {
            format!("Failed to remove helper files directory: {:?}", r_files_dir)
        })?;
        info!(
            "  ✓ Removed helper files directory: {:?}",
            Defaults::LANGUAGE_FILES_DIR
        );
    }

    // 4. Remove generated .typ and .pdf files
    if let Some(main_file_name) = config.document.main {
        let main_stem = Path::new(&main_file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");

        // Remove {stem}.typ and {stem}.pdf (e.g., main.typ, main.pdf)
        let typ_file = project_root.join(format!("{}.typ", main_stem));
        let pdf_file = project_root.join(format!("{}.pdf", main_stem));

        if typ_file.exists() {
            summary.inspect(&typ_file, false)?;
            fs::remove_file(&typ_file)
                .with_context(|| format!("Failed to remove file: {:?}", typ_file))?;
            info!("  ✓ Removed {}.typ", main_stem);
        }

        if pdf_file.exists() {
            summary.inspect(&pdf_file, false)?;
            fs::remove_file(&pdf_file)
                .with_context(|| format!("Failed to remove file: {:?}", pdf_file))?;
            info!("  ✓ Removed {}.pdf", main_stem);
        }
    }

    // Also remove any hidden .*.typ and .*.pdf files (legacy or intermediate files)
    let entries = fs::read_dir(&project_root)?;
    for entry in entries {
        let path = entry?.path();
        let filename = path.file_name().and_then(|n| n.to_str());

        match (path.is_file(), filename) {
            (true, Some(name))
                if name.starts_with('.') && (name.ends_with(".typ") || name.ends_with(".pdf")) =>
            {
                summary.inspect(&path, false)?;
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove file: {:?}", path))?;
                info!("  ✓ Removed legacy file: {}", name);
            }
            _ => {}
        }
    }

    Ok(summary)
}
