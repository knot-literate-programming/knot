// Cache Module
//
// Provides cache management for chunk and inline expression results:
// - Content-addressed storage with SHA256 hashing
// - Sequential invalidation (chunk N+1 depends on chunk N)
// - Dependency tracking (file mtime/size)
// - Persistent metadata (metadata.json)

mod metadata;
mod hashing;
mod storage;

pub use metadata::{CacheMetadata, ChunkCacheEntry, InlineCacheEntry};
pub use hashing::hash_dependencies;

use crate::executors::ExecutionResult;
use crate::parser::ChunkOptions;
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

pub struct Cache {
    pub cache_dir: PathBuf,
    pub metadata: CacheMetadata,
}

impl Cache {
    /// Creates a new cache instance
    ///
    /// Creates cache directory if it doesn't exist and loads existing metadata
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_dir)?;
        let metadata = storage::load_metadata(&cache_dir);

        Ok(Self {
            cache_dir,
            metadata,
        })
    }

    /// Computes hash for a chunk (using sequential chaining and dependencies)
    pub fn get_chunk_hash(
        &self,
        code: &str,
        options: &ChunkOptions,
        previous_hash: &str,
        dependencies_hash: &str,
    ) -> String {
        hashing::get_chunk_hash(code, options, previous_hash, dependencies_hash)
    }

    /// Computes hash for an inline expression (using sequential chaining)
    pub fn get_inline_expr_hash(
        &self,
        code: &str,
        options: &crate::parser::InlineOptions,
        previous_hash: &str,
    ) -> String {
        hashing::get_inline_expr_hash(code, options, previous_hash)
    }

    /// Check if inline result is cached
    pub fn has_cached_inline_result(&self, hash: &str) -> bool {
        self.metadata
            .inline_expressions
            .iter()
            .any(|entry| entry.hash == hash)
    }

    /// Get cached inline result
    pub fn get_cached_inline_result(&self, hash: &str) -> Result<String> {
        let entry = self
            .metadata
            .inline_expressions
            .iter()
            .find(|e| e.hash == hash)
            .ok_or_else(|| anyhow!("Inline cache entry with hash {} not found", hash))?;
        Ok(entry.result.clone())
    }

    /// Save inline expression result to cache
    pub fn save_inline_result(&mut self, hash: String, result: &str) -> Result<()> {
        let new_entry = InlineCacheEntry {
            hash: hash.clone(),
            result: result.to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Remove old entry if exists
        self.metadata
            .inline_expressions
            .retain(|entry| entry.hash != hash);
        self.metadata.inline_expressions.push(new_entry);

        storage::save_metadata(&self.cache_dir, &self.metadata)?;
        Ok(())
    }

    /// Check if chunk result is cached
    pub fn has_cached_result(&self, hash: &str) -> bool {
        self.metadata.chunks.iter().any(|entry| entry.hash == hash)
    }

    /// Get cached chunk result
    pub fn get_cached_result(&self, hash: &str) -> Result<ExecutionResult> {
        storage::get_cached_result(&self.cache_dir, hash, &self.metadata)
    }

    /// Save chunk execution result to cache
    pub fn save_result(
        &mut self,
        chunk_index: usize,
        chunk_name: Option<String>,
        hash: String,
        result: &ExecutionResult,
        dependencies: Vec<PathBuf>,
    ) -> Result<()> {
        let files_to_cache = storage::save_result(
            &self.cache_dir,
            chunk_index,
            chunk_name.clone(),
            hash.clone(),
            result,
            dependencies.clone(),
        )?;

        // Don't cache empty results
        if files_to_cache.is_empty() {
            return Ok(());
        }

        let new_entry = storage::create_chunk_entry(
            chunk_index,
            chunk_name,
            hash,
            files_to_cache,
            dependencies,
        );

        // Remove old entry if it exists
        self.metadata
            .chunks
            .retain(|entry| entry.index != chunk_index);
        self.metadata.chunks.push(new_entry);

        storage::save_metadata(&self.cache_dir, &self.metadata)?;
        Ok(())
    }
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
        let hash1 = Cache::new(tmp_dir.path().to_path_buf())
            .unwrap()
            .get_chunk_hash("read.csv('data.csv')", &opts, "", &deps_hash1);

        // Modifier fichier
        thread::sleep(Duration::from_millis(10));
        fs::write(&tmp_file, "a,b\n3,4").unwrap();

        let deps_hash2 = hash_dependencies(&opts.depends).unwrap();
        let hash2 = Cache::new(tmp_dir.path().to_path_buf())
            .unwrap()
            .get_chunk_hash("read.csv('data.csv')", &opts, "", &deps_hash2);

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
