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
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
    pub inline_expressions: Vec<InlineCacheEntry>,
    #[serde(default)]
    pub freeze_objects: HashMap<String, FreezeObjectInfo>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChunkCacheEntry {
    pub index: usize,
    pub name: Option<String>,
    pub language: String,
    pub hash: String,
    pub files: Vec<String>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FreezeObjectInfo {
    pub name: String,             // Variable name in the language environment
    pub hash: String,             // xxHash64 of the object content
    pub size_bytes: u64,          // Size in bytes
    pub language: String,         // "r", "python", "julia"
    pub created_in_chunk: String, // Chunk name or index
    pub created_at: String,       // Timestamp (RFC3339)
}

/// Files and frozen-object bindings belonging to one exact interpreter state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotEntry {
    pub reusable: bool,
    pub files: HashMap<String, String>,
    pub freeze_objects: HashMap<String, FreezeObjectInfo>,
}

pub const CACHE_FORMAT_VERSION: u32 = 2;

impl Default for CacheMetadata {
    fn default() -> Self {
        Self {
            format_version: CACHE_FORMAT_VERSION,
            document_hash: String::new(),
            chunks: Vec::new(),
            inline_expressions: Vec::new(),
            freeze_objects: HashMap::new(),
            snapshots: HashMap::new(),
        }
    }
}
