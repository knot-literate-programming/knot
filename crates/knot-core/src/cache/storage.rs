// Cache Storage Operations
//
// Handles file I/O for cache:
// - Loading/saving cache metadata (metadata.json)
// - Saving chunk execution results to cache files
// - Loading cached results from files

use super::metadata::{CacheMetadata, ChunkCacheEntry};
use crate::executors::ExecutionResult;
use anyhow::{anyhow, Result};
use chrono::Utc;
use log::warn;
use std::fs;
use std::path::{Path, PathBuf};

/// Loads cache metadata from disk
///
/// Returns default metadata if file doesn't exist or is corrupt
pub fn load_metadata(cache_dir: &Path) -> CacheMetadata {
    let metadata_path = cache_dir.join("metadata.json");

    if metadata_path.exists() {
        let content = match fs::read_to_string(&metadata_path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read cache metadata: {}", e);
                return CacheMetadata::default();
            }
        };

        match serde_json::from_str(&content) {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!(
                    "Failed to parse cache metadata ({:?}). Ignoring cache. Error: {}",
                    metadata_path, e
                );
                CacheMetadata::default()
            }
        }
    } else {
        CacheMetadata::default()
    }
}

/// Saves cache metadata to disk
pub fn save_metadata(cache_dir: &Path, metadata: &CacheMetadata) -> Result<()> {
    let metadata_path = cache_dir.join("metadata.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(metadata_path, content)?;
    Ok(())
}

/// Retrieves cached chunk result from disk
///
/// Verifies that all referenced files exist before reconstructing the result
pub fn get_cached_result(cache_dir: &Path, hash: &str, metadata: &CacheMetadata) -> Result<ExecutionResult> {
    let entry = metadata
        .chunks
        .iter()
        .find(|e| e.hash == hash)
        .ok_or_else(|| anyhow!("Cache entry with hash {} not found", hash))?;

    // Handle chunks with no output files (e.g., assignments without print)
    if entry.files.is_empty() {
        return Ok(ExecutionResult::Text(String::new()));
    }

    // Verify all files exist
    for file in &entry.files {
        let path = cache_dir.join(file);
        if !path.exists() {
            return Err(anyhow!("Cache file missing: {:?}", path));
        }
    }

    // Reconstruct result based on file types
    // For now, we handle single file results (Text, Plot, or DataFrame)
    // The combined cases (TextAndPlot, DataFrameAndPlot) will need more logic
    let result_path = cache_dir.join(&entry.files[0]);
    let ext = result_path.extension().and_then(|e| e.to_str());

    match ext {
        Some("txt") => {
            let text = fs::read_to_string(&result_path)?;
            Ok(ExecutionResult::Text(text))
        }
        Some("svg") | Some("png") => Ok(ExecutionResult::Plot(result_path)),
        Some("csv") => Ok(ExecutionResult::DataFrame(result_path)),
        _ => Err(anyhow!("Unknown cache file type: {:?}", result_path)),
    }
}

/// Saves chunk execution result to cache
///
/// Creates necessary files in cache directory and updates metadata
pub fn save_result(
    cache_dir: &Path,
    _chunk_index: usize,
    _chunk_name: Option<String>,
    hash: String,
    result: &ExecutionResult,
    _dependencies: Vec<PathBuf>,
) -> Result<Vec<String>> {
    let files_to_cache = match result {
        ExecutionResult::Text(text) if !text.trim().is_empty() => {
            let filename = format!("chunk_{}.txt", hash);
            let path = cache_dir.join(&filename);
            fs::write(&path, text)?;
            vec![filename]
        }
        ExecutionResult::Plot(plot_path) => {
            // Assuming the plot is already in the cache dir, just get its name
            let filename = plot_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            vec![filename]
        }
        ExecutionResult::DataFrame(csv_path) => {
            // DataFrame CSV is already saved in the cache dir, just get its name
            let filename = csv_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            vec![filename]
        }
        ExecutionResult::TextAndPlot { text, plot } => {
            let text_filename = format!("chunk_{}.txt", hash);
            let text_path = cache_dir.join(&text_filename);
            fs::write(&text_path, text)?;

            let plot_filename = plot.file_name().unwrap().to_string_lossy().to_string();
            vec![text_filename, plot_filename]
        }
        ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
            let dataframe_filename = dataframe
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let plot_filename = plot.file_name().unwrap().to_string_lossy().to_string();
            vec![dataframe_filename, plot_filename]
        }
        _ => {
            // Don't cache empty results
            return Ok(Vec::new());
        }
    };

    Ok(files_to_cache)
}

/// Creates a new ChunkCacheEntry
pub fn create_chunk_entry(
    chunk_index: usize,
    chunk_name: Option<String>,
    hash: String,
    files: Vec<String>,
    dependencies: Vec<PathBuf>,
) -> ChunkCacheEntry {
    ChunkCacheEntry {
        index: chunk_index,
        name: chunk_name,
        hash,
        files,
        dependencies: dependencies
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        updated_at: Utc::now().to_rfc3339(),
    }
}
