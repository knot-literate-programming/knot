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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_csv_to_cache() {
        let temp_dir = TempDir::new().unwrap();
        let csv_content = "a,b,c\n1,2,3\n4,5,6";

        let result = save_csv_to_cache(csv_content, temp_dir.path()).unwrap();

        // Check file exists
        assert!(result.exists());

        // Check filename format
        let filename = result.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("dataframe_"));
        assert!(filename.ends_with(".csv"));

        // Check content matches
        let saved_content = std::fs::read_to_string(&result).unwrap();
        assert_eq!(saved_content, csv_content);
    }

    #[test]
    fn test_save_csv_hash_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let csv_content = "x,y\n1,2\n3,4";

        // Save same content twice
        let result1 = save_csv_to_cache(csv_content, temp_dir.path()).unwrap();
        let result2 = save_csv_to_cache(csv_content, temp_dir.path()).unwrap();

        // Should produce same filename (same hash)
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_save_csv_different_content() {
        let temp_dir = TempDir::new().unwrap();

        let result1 = save_csv_to_cache("a,b\n1,2", temp_dir.path()).unwrap();
        let result2 = save_csv_to_cache("a,b\n3,4", temp_dir.path()).unwrap();

        // Different content should produce different filenames
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_save_csv_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let csv_content = "";

        let result = save_csv_to_cache(csv_content, temp_dir.path()).unwrap();

        assert!(result.exists());
        let saved_content = std::fs::read_to_string(&result).unwrap();
        assert_eq!(saved_content, "");
    }

    #[test]
    fn test_copy_plot_to_cache_svg() {
        let temp_dir = TempDir::new().unwrap();

        // Create a source SVG file
        let source_path = temp_dir.path().join("plot.svg");
        let svg_content = r#"<svg><circle cx="50" cy="50" r="40"/></svg>"#;
        std::fs::write(&source_path, svg_content).unwrap();

        let result = copy_plot_to_cache(&source_path, temp_dir.path()).unwrap();

        // Check file exists
        assert!(result.exists());

        // Check extension preserved
        assert_eq!(result.extension().unwrap(), "svg");

        // Check filename format
        let filename = result.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("plot_"));
        assert!(filename.ends_with(".svg"));

        // Check content matches
        let saved_content = std::fs::read_to_string(&result).unwrap();
        assert_eq!(saved_content, svg_content);
    }

    #[test]
    fn test_copy_plot_to_cache_png() {
        let temp_dir = TempDir::new().unwrap();

        // Create a source PNG file (fake binary content)
        let source_path = temp_dir.path().join("plot.png");
        let png_content = vec![0x89, 0x50, 0x4E, 0x47]; // PNG header
        std::fs::write(&source_path, &png_content).unwrap();

        let result = copy_plot_to_cache(&source_path, temp_dir.path()).unwrap();

        // Check file exists
        assert!(result.exists());

        // Check extension preserved
        assert_eq!(result.extension().unwrap(), "png");

        // Check content matches
        let saved_content = std::fs::read(&result).unwrap();
        assert_eq!(saved_content, png_content);
    }

    #[test]
    fn test_copy_plot_hash_consistency() {
        let temp_dir = TempDir::new().unwrap();

        // Create source file
        let source_path = temp_dir.path().join("plot.svg");
        std::fs::write(&source_path, "<svg></svg>").unwrap();

        // Copy same file twice
        let result1 = copy_plot_to_cache(&source_path, temp_dir.path()).unwrap();
        let result2 = copy_plot_to_cache(&source_path, temp_dir.path()).unwrap();

        // Should produce same filename (same hash)
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_copy_plot_different_content() {
        let temp_dir = TempDir::new().unwrap();

        // Create two different source files
        let source1 = temp_dir.path().join("plot1.svg");
        let source2 = temp_dir.path().join("plot2.svg");
        std::fs::write(&source1, "<svg>A</svg>").unwrap();
        std::fs::write(&source2, "<svg>B</svg>").unwrap();

        let result1 = copy_plot_to_cache(&source1, temp_dir.path()).unwrap();
        let result2 = copy_plot_to_cache(&source2, temp_dir.path()).unwrap();

        // Different content should produce different filenames
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_copy_plot_no_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create source file without extension
        let source_path = temp_dir.path().join("plot");
        std::fs::write(&source_path, "content").unwrap();

        let result = copy_plot_to_cache(&source_path, temp_dir.path()).unwrap();

        // Should default to .svg
        assert_eq!(result.extension().unwrap(), "svg");
    }

    #[test]
    fn test_copy_plot_nonexistent_source() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("nonexistent.svg");

        let result = copy_plot_to_cache(&source_path, temp_dir.path());

        // Should fail
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read plot file"));
    }
}
