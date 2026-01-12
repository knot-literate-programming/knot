// Cache File Management
//
// Handles saving and copying files to the cache directory:
// - CSV files from dataframes (content-based hash naming)
// - Plot files (content-based hash naming with extension preservation)

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Saves CSV content to cache with SHA256-based filename
pub fn save_csv_to_cache(csv_content: &str, cache_dir: &Path) -> Result<PathBuf> {
    let mut hasher = Sha256::new();
    hasher.update(csv_content.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let filename = format!("dataframe_{}.csv", &hash[..16]);
    let csv_path = cache_dir.join(&filename);

    std::fs::write(&csv_path, csv_content).context("Failed to write CSV to cache")?;

    Ok(csv_path)
}

/// Copies plot file to cache with content-based naming and extension preservation
pub fn copy_plot_to_cache(source_path: &Path, cache_dir: &Path) -> Result<PathBuf> {
    // Read the plot file to compute hash
    let plot_content = std::fs::read(source_path).context("Failed to read plot file")?;

    let mut hasher = Sha256::new();
    hasher.update(&plot_content);
    let hash = format!("{:x}", hasher.finalize());

    // Preserve the file extension
    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("svg");

    let filename = format!("plot_{}.{}", &hash[..16], extension);
    let dest_path = cache_dir.join(&filename);

    // Copy the file to cache
    std::fs::copy(source_path, &dest_path).context("Failed to copy plot to cache")?;

    Ok(dest_path)
}
