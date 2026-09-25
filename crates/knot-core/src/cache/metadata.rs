#![allow(missing_docs)]
// Cache Metadata Structures
//
// Defines the data structures for cache metadata:
// - CacheMetadata: Root structure with document hash and entries
// - ChunkCacheEntry: Metadata for cached chunk results
// - InlineCacheEntry: Metadata for cached inline expression results

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct CacheMetadata {
    #[serde(default)]
    pub format_version: u32,
    #[serde(default)]
    pub snapshots: HashMap<String, SnapshotEntry>,
    #[serde(default)]
    pub disabled_snapshot_languages: std::collections::HashSet<String>,
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
    pub inline_expressions: Vec<InlineCacheEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChunkCacheEntry {
    pub index: usize,
    pub name: Option<String>,
    pub language: String,
    pub hash: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub exports: Vec<crate::executors::DataExport>,
    pub file_hashes: HashMap<String, String>,
    #[serde(default)]
    pub warnings: Vec<crate::executors::side_channel::RuntimeWarning>,
    #[serde(default)]
    pub error: Option<crate::executors::side_channel::RuntimeError>,
    pub dependencies: Vec<String>,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InlineCacheEntry {
    pub hash: String,
    pub result: String,
    pub updated_at: String,
}

/// Files belonging to one exact interpreter state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotEntry {
    pub reusable: bool,
    /// SHA-256 of each snapshot file, checked when the snapshot is restored.
    pub files: HashMap<String, String>,
    /// Size and modification time of each file when its hash was recorded or
    /// last verified: planning compares these instead of re-hashing the files
    /// on every compilation. Absent in older caches, which are then hashed.
    #[serde(default)]
    pub stats: HashMap<String, FileStat>,
}

/// Cheap identity of a file: a change of content normally changes one of these.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FileStat {
    pub len: u64,
    pub modified: std::time::SystemTime,
}

impl FileStat {
    pub fn of(path: &std::path::Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

pub const CACHE_FORMAT_VERSION: u32 = 5;

impl Default for CacheMetadata {
    fn default() -> Self {
        Self {
            format_version: CACHE_FORMAT_VERSION,
            document_hash: String::new(),
            chunks: Vec::new(),
            inline_expressions: Vec::new(),
            snapshots: HashMap::new(),
            disabled_snapshot_languages: Default::default(),
        }
    }
}
