// Cache Metadata Structures
//
// Defines the data structures for cache metadata:
// - CacheMetadata: Root structure with document hash and entries
// - ChunkCacheEntry: Metadata for cached chunk results
// - InlineCacheEntry: Metadata for cached inline expression results

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CacheMetadata {
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
    pub inline_expressions: Vec<InlineCacheEntry>,
    #[serde(default)]
    pub constant_objects: HashMap<String, ConstantObjectInfo>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct InlineCacheEntry {
    pub hash: String,
    pub result: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConstantObjectInfo {
    pub hash: String,                  // xxHash64 of the object content
    pub size_bytes: u64,               // Size in bytes
    pub language: String,              // "r", "python", "julia"
    pub created_in_chunk: String,      // Chunk name or index
    pub created_at: String,            // Timestamp (RFC3339)
}
