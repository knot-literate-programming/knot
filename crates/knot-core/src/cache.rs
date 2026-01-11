use crate::executors::ExecutionResult;
use crate::parser::ChunkOptions;
use anyhow::{anyhow, Result};
use chrono::Utc;
use log::warn;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

// NOTE : Ces structures sont basées sur la section 7.4 du document de référence.

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CacheMetadata {
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChunkCacheEntry {
    pub index: usize,
    pub name: Option<String>,
    pub hash: String,
    pub files: Vec<String>,
    pub dependencies: Vec<String>,
    pub updated_at: String,
}

pub struct Cache {
    pub cache_dir: PathBuf,
    pub metadata: CacheMetadata,
}

impl Cache {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_dir)?;

        let metadata_path = cache_dir.join("metadata.json");
        let metadata = if metadata_path.exists() {
            let content = fs::read_to_string(&metadata_path)?;
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
        };

        Ok(Self {
            cache_dir,
            metadata,
        })
    }

    pub fn get_chunk_hash(
        &self,
        code: &str,
        options: &ChunkOptions,
        previous_hash: &str,
        dependencies_hash: &str,
    ) -> String {
        let options_str = serde_json::to_string(options).unwrap_or_default();
        let chunk_content = format!(
            "{}|{}|{}|{}",
            code, options_str, previous_hash, dependencies_hash
        );

        let mut hasher = Sha256::new();
        hasher.update(chunk_content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn has_cached_result(&self, hash: &str) -> bool {
        self.metadata.chunks.iter().any(|entry| entry.hash == hash)
    }

    pub fn get_cached_result(&self, hash: &str) -> Result<ExecutionResult> {
        let entry = self.metadata
            .chunks
            .iter()
            .find(|e| e.hash == hash)
            .ok_or_else(|| anyhow!("Cache entry with hash {} not found", hash))?;

        for file in &entry.files {
            let path = self.cache_dir.join(file);
            if !path.exists() {
                return Err(anyhow!("Cache file missing: {:?}", path));
            }
        }

        // For now, we only handle single file results (Text, Plot, or DataFrame).
        // The Both case will need to be handled more robustly.
        let result_path = self.cache_dir.join(&entry.files[0]);
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

    pub fn save_result(
        &mut self,
        chunk_index: usize,
        chunk_name: Option<String>,
        hash: String,
        result: &ExecutionResult,
        dependencies: Vec<PathBuf>,
    ) -> Result<()> {
        let files_to_cache = match result {
            ExecutionResult::Text(text) if !text.trim().is_empty() => {
                let filename = format!("chunk_{}.txt", hash);
                let path = self.cache_dir.join(&filename);
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
            ExecutionResult::Both { text, plot } => {
                let text_filename = format!("chunk_{}.txt", hash);
                let text_path = self.cache_dir.join(&text_filename);
                fs::write(&text_path, text)?;

                let plot_filename = plot.file_name().unwrap().to_string_lossy().to_string();
                vec![text_filename, plot_filename]
            }
            _ => {
                // Don't cache empty results
                return Ok(());
            }
        };

        let new_entry = ChunkCacheEntry {
            index: chunk_index,
            name: chunk_name,
            hash,
            files: files_to_cache,
            dependencies: dependencies
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Remove old entry if it exists
        self.metadata.chunks.retain(|entry| entry.index != chunk_index);
        self.metadata.chunks.push(new_entry);

        self.save_metadata()?;
        Ok(())
    }

    fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.cache_dir.join("metadata.json");
        let content = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(metadata_path, content)?;
        Ok(())
    }
}

pub fn hash_dependencies(depends: &[PathBuf]) -> Result<String> {
    if depends.is_empty() {
        return Ok(String::new());
    }

    let mut hasher = Sha256::new();

    for path in depends {
        if !path.exists() {
            return Err(anyhow!("Dependency not found: {:?}", path));
        }

        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;

        // Hash: path + modified_time + size
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(format!("{:?}", modified).as_bytes());
        hasher.update(metadata.len().to_string().as_bytes());
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn test_hash_chaining_basic() {
        let tmp_dir = tempdir().unwrap();
        let cache = Cache::new(tmp_dir.path().to_path_buf()).unwrap();
        let opts = ChunkOptions::default();

        let hash1 = cache.get_chunk_hash("x <- 1", &opts, "", "");
        let hash2 = cache.get_chunk_hash("y <- x + 1", &opts, &hash1, "");
        let hash3 = cache.get_chunk_hash("z <- y * 2", &opts, &hash2, "");

        // Changer chunk 1 invalide tout
        let hash1_mod = cache.get_chunk_hash("x <- 2", &opts, "", "");
        let hash2_after = cache.get_chunk_hash("y <- x + 1", &opts, &hash1_mod, "");
        let hash3_after = cache.get_chunk_hash("z <- y * 2", &opts, &hash2_after, "");

        assert_ne!(hash1, hash1_mod);
        assert_ne!(hash2, hash2_after);
        assert_ne!(hash3, hash3_after);
    }

    #[test]
    fn test_dependency_invalidation() {
        let tmp_dir = tempdir().unwrap();
        let tmp_file = tmp_dir.path().join("data.csv");
        fs::write(&tmp_file, "a,b\n1,2").unwrap();

        let opts = ChunkOptions {
            depends: vec![tmp_file.clone()],
            ..Default::default()
        };

        let deps_hash1 = hash_dependencies(&opts.depends).unwrap();
        let hash1 =
            Cache::new(tmp_dir.path().to_path_buf()).unwrap().get_chunk_hash(
                "read.csv('data.csv')",
                &opts,
                "",
                &deps_hash1,
            );

        // Modifier fichier
        thread::sleep(Duration::from_millis(10));
        fs::write(&tmp_file, "a,b\n3,4").unwrap();

        let deps_hash2 = hash_dependencies(&opts.depends).unwrap();
        let hash2 =
            Cache::new(tmp_dir.path().to_path_buf()).unwrap().get_chunk_hash(
                "read.csv('data.csv')",
                &opts,
                "",
                &deps_hash2,
            );

        assert_ne!(deps_hash1, deps_hash2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_options_affect_hash() {
        let tmp_dir = tempdir().unwrap();
        let cache = Cache::new(tmp_dir.path().to_path_buf()).unwrap();

        let opts1 = ChunkOptions {
            eval: true,
            ..Default::default()
        };
        let opts2 = ChunkOptions {
            eval: false,
            ..Default::default()
        };

        let hash1 = cache.get_chunk_hash("x <- 1", &opts1, "", "");
        let hash2 = cache.get_chunk_hash("x <- 1", &opts2, "", "");

        assert_ne!(hash1, hash2);
    }
}
