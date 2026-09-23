//! Content-addressed cache for chunk and inline expression results.
//!
//! Results are keyed by a SHA-256 hash that chains sequential chunk hashes,
//! so editing chunk N automatically invalidates N+1, N+2, … on the next run.
//! File dependencies are also tracked (path + content hash) so that changing
//! an external data file triggers re-execution of the chunks that read it.
//!
//! Metadata is persisted to `metadata.json` inside the cache directory and
//! re-loaded on the next compilation without re-executing unchanged chunks.

/// SHA-256 hashing utilities for chunks, inline expressions and file dependencies.
pub mod hashing;
mod metadata;
mod storage;

pub use hashing::hash_dependencies;
pub use metadata::{
    CacheMetadata, ChunkCacheEntry, FreezeObjectInfo, InlineCacheEntry, SnapshotEntry,
};

use crate::executors::{ExecutionAttempt, ExecutionOutput};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

/// On-disk cache for a single `.knot` document.
///
/// Stores chunk results, inline expression results, session snapshots and
/// freeze-object hashes.  All content-addressed entries live under
/// `cache_dir`; the index is persisted as `metadata.json` in the same directory.
pub struct Cache {
    /// Path to the cache directory (e.g. `.knot_cache/main/`).
    pub cache_dir: PathBuf,
    /// In-memory index of all cached entries; written to disk by [`Cache::save_metadata`].
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
    /// Computes hash for an inline expression (using sequential chaining)
    pub fn get_inline_expr_hash(
        &self,
        language: &str,
        code: &str,
        options: &crate::parser::InlineOptions,
        previous_hash: &str,
    ) -> String {
        hashing::get_inline_expr_hash(language, code, options, previous_hash)
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

    /// Save a chunk execution error to cache
    pub fn save_error(
        &mut self,
        chunk_index: usize,
        chunk_name: Option<String>,
        language: String,
        hash: String,
        error: crate::executors::side_channel::RuntimeError,
        dependencies: Vec<PathBuf>,
    ) -> Result<()> {
        self.save_chunk_entry(ChunkCacheEntry {
            error: Some(error),
            ..Self::chunk_entry(chunk_index, chunk_name, language, hash, dependencies)
        })
    }

    /// Get cached chunk result
    pub fn get_cached_result(&self, hash: &str) -> Result<ExecutionAttempt> {
        storage::get_cached_result(&self.cache_dir, hash, &self.metadata)
    }

    /// Save chunk execution result to cache
    pub fn save_result(
        &mut self,
        chunk_index: usize,
        chunk_name: Option<String>,
        language: String,
        hash: String,
        output: &ExecutionOutput,
        dependencies: Vec<PathBuf>,
    ) -> Result<()> {
        let files_to_cache = storage::save_result(&self.cache_dir, &hash, output)?;

        let file_hashes = files_to_cache
            .iter()
            .map(|file| {
                Ok((
                    file.clone(),
                    hashing::hash_file(&self.cache_dir.join(file))?,
                ))
            })
            .collect::<Result<_>>()?;

        // Record successful execution even when there are no output files.
        self.save_chunk_entry(ChunkCacheEntry {
            files: files_to_cache,
            file_hashes,
            warnings: output.warnings.clone(),
            ..Self::chunk_entry(chunk_index, chunk_name, language, hash, dependencies)
        })
    }

    fn chunk_entry(
        index: usize,
        name: Option<String>,
        language: String,
        hash: String,
        dependencies: Vec<PathBuf>,
    ) -> ChunkCacheEntry {
        ChunkCacheEntry {
            index,
            name,
            language,
            hash,
            files: Vec::new(),
            file_hashes: Default::default(),
            warnings: Vec::new(),
            error: None,
            dependencies: dependencies
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    fn save_chunk_entry(&mut self, entry: ChunkCacheEntry) -> Result<()> {
        self.metadata.chunks.retain(|old| old.index != entry.index);
        self.metadata.chunks.push(entry);
        self.save_metadata()
    }

    /// Get the path where a snapshot file should be stored for a given hash and extension
    ///
    /// # Arguments
    /// * `node_hash` - The hash of the chunk or inline expression
    /// * `extension` - The file extension (e.g., "RData", "pkl")
    pub fn get_snapshot_path(&self, node_hash: &str, extension: &str) -> PathBuf {
        self.cache_dir
            .join(format!("snapshot_{}.{}", node_hash, extension))
    }

    /// Whether every file required to restore a snapshot is intact.
    pub fn snapshot_is_valid(&self, hash: &str) -> bool {
        self.metadata.snapshots.get(hash).is_some_and(|entry| {
            entry.reusable
                && entry.freeze_objects.is_empty()
                && !entry.files.is_empty()
                && entry.files.iter().all(|(path, expected)| {
                    hashing::hash_file(&self.cache_dir.join(path))
                        .is_ok_and(|actual| actual == *expected)
                })
        })
    }

    /// Validate and restore a complete session; snapshots with frozen bindings are rejected.
    /// Callers choose the snapshot and manage the interpreter lifetime.
    pub fn restore_snapshot(
        &self,
        hash: &str,
        executor: &mut dyn crate::executors::KnotExecutor,
    ) -> Result<()> {
        if !self.snapshot_is_valid(hash) {
            bail!(
                "Cannot restore snapshot {hash}: missing, changed or non-reusable; rebuild the document"
            );
        }
        let path = self.get_snapshot_path(hash, executor.snapshot_extension());
        executor
            .load_session(&path)
            .with_context(|| format!("Failed to restore snapshot {}", path.display()))
    }

    /// Save the cache metadata to disk
    ///
    /// Writes the metadata (including constant objects info) to metadata.json
    pub fn save_metadata(&self) -> Result<()> {
        storage::save_metadata(&self.cache_dir, &self.metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_cache_dir;
    use crate::parser::ChunkOptions;
    use tempfile::tempdir;

    #[test]
    fn test_hash_chaining_basic() {
        let tmp_dir = tempdir().unwrap();
        let project_root = tmp_dir.path();
        let cache_dir = get_cache_dir(project_root, "test");
        let _cache = Cache::new(cache_dir).unwrap();
        let opts = ChunkOptions::default();

        let hash1 = hashing::get_chunk_hash("r", "x <- 1", &opts, "", "");
        let hash2 = hashing::get_chunk_hash("r", "y <- x + 1", &opts, &hash1, "");
        let hash3 = hashing::get_chunk_hash("r", "z <- y * 2", &opts, &hash2, "");

        // Changer chunk 1 invalide tout
        let hash1_mod = hashing::get_chunk_hash("r", "x <- 2", &opts, "", "");
        let hash2_after = hashing::get_chunk_hash("r", "y <- x + 1", &opts, &hash1_mod, "");
        let hash3_after = hashing::get_chunk_hash("r", "z <- y * 2", &opts, &hash2_after, "");

        assert_ne!(hash1, hash1_mod);
        assert_ne!(hash2, hash2_after);
        assert_ne!(hash3, hash3_after);
    }

    #[test]
    fn test_dependency_invalidation() {
        let tmp_dir = tempdir().unwrap();
        let project_root = tmp_dir.path();
        let _cache_dir = get_cache_dir(project_root, "test");
        let tmp_file = tmp_dir.path().join("data.csv");
        fs::write(&tmp_file, "a,b\n1,2").unwrap();

        let opts = ChunkOptions {
            depends: vec![tmp_file.clone()],
            ..Default::default()
        };

        let deps_hash1 = hash_dependencies(&opts.depends).unwrap();
        let hash1 = hashing::get_chunk_hash("r", "read.csv('data.csv')", &opts, "", &deps_hash1);

        // Modify file — content hashing detects the change immediately
        fs::write(&tmp_file, "a,b\n3,4").unwrap();

        let deps_hash2 = hash_dependencies(&opts.depends).unwrap();
        let hash2 = hashing::get_chunk_hash("r", "read.csv('data.csv')", &opts, "", &deps_hash2);

        assert_ne!(deps_hash1, deps_hash2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_options_affect_hash() {
        let tmp_dir = tempdir().unwrap();
        let project_root = tmp_dir.path();
        let cache_dir = get_cache_dir(project_root, "test");
        let _cache = Cache::new(cache_dir).unwrap();

        let opts1 = ChunkOptions {
            eval: Some(true),
            ..Default::default()
        };
        let opts2 = ChunkOptions {
            eval: Some(false),
            ..Default::default()
        };

        let hash1 = hashing::get_chunk_hash("r", "x <- 1", &opts1, "", "");
        let hash2 = hashing::get_chunk_hash("r", "x <- 1", &opts2, "", "");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_snapshot_path_generation() {
        let tmp_dir = tempdir().unwrap();
        let project_root = tmp_dir.path();
        let cache_dir = get_cache_dir(project_root, "test");
        let cache = Cache::new(cache_dir.clone()).unwrap();

        let hash = "abc123def456";
        let snapshot_path = cache.get_snapshot_path(hash, "RData");

        // Check path format
        assert_eq!(
            snapshot_path.file_name().unwrap().to_str().unwrap(),
            "snapshot_abc123def456.RData"
        );

        // Check parent directory
        assert_eq!(snapshot_path.parent().unwrap(), cache_dir);
    }

    #[test]
    fn test_snapshot_different_hashes() {
        let tmp_dir = tempdir().unwrap();
        let project_root = tmp_dir.path();
        let cache_dir = get_cache_dir(project_root, "test");
        let cache = Cache::new(cache_dir).unwrap();

        let hash1 = "hash1";
        let hash2 = "hash2";

        let path1 = cache.get_snapshot_path(hash1, "RData");
        let path2 = cache.get_snapshot_path(hash2, "RData");

        // Different hashes should give different paths
        assert_ne!(path1, path2);
    }
    #[test]
    fn chunk_entry_replacement_persists_success_and_error_transitions() {
        use crate::executors::{ExecutionResult, RuntimeError};
        let root = tempdir().unwrap();
        let mut cache = Cache::new(root.path().to_path_buf()).unwrap();
        let output = ExecutionOutput {
            result: ExecutionResult::Text("answer".into()),
            warnings: vec![],
        };
        cache
            .save_result(
                0,
                Some("first".into()),
                "python".into(),
                "success".into(),
                &output,
                vec![],
            )
            .unwrap();
        let error = RuntimeError {
            message: Some("failed".into()),
            call: None,
            line: None,
            traceback: vec![],
        };
        cache
            .save_error(
                0,
                Some("second".into()),
                "python".into(),
                "error".into(),
                error,
                vec![],
            )
            .unwrap();
        let persisted = Cache::new(root.path().to_path_buf()).unwrap();
        assert_eq!(persisted.metadata.chunks.len(), 1);
        assert_eq!(persisted.metadata.chunks[0].name.as_deref(), Some("second"));
        assert!(persisted.get_cached_result("success").is_err());
        assert!(matches!(
            persisted.get_cached_result("error").unwrap(),
            ExecutionAttempt::RuntimeError(_)
        ));
        cache
            .save_result(
                0,
                None,
                "python".into(),
                "recovered".into(),
                &output,
                vec![],
            )
            .unwrap();
        let persisted = Cache::new(root.path().to_path_buf()).unwrap();
        assert_eq!(persisted.metadata.chunks.len(), 1);
        assert!(persisted.get_cached_result("error").is_err());
        assert!(matches!(
            persisted.get_cached_result("recovered").unwrap(),
            ExecutionAttempt::Success(_)
        ));
    }
}
