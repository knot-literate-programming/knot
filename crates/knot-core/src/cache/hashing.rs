// Cache Hashing Logic
//
// Handles SHA256-based hashing for:
// - Chunk code with options and dependencies (with chaining)
// - Inline expressions with options
// - File dependencies (path + mtime + size)

use crate::parser::ChunkOptions;
use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

/// Computes SHA256 hash for a chunk
///
/// Hash includes:
/// - Code content
/// - Serialized options (eval, show, cache, graphics, etc.)
/// - Previous chunk hash (for sequential invalidation)
/// - Dependencies hash (for file-based invalidation)
pub fn get_chunk_hash(
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

/// Computes SHA256 hash for an inline expression
///
/// Hash includes:
/// - Code content
/// - Options (show, eval, digits)
/// - Previous inline expression hash (for sequential invalidation)
pub fn get_inline_expr_hash(
    code: &str,
    options: &crate::parser::InlineOptions,
    previous_hash: &str,
) -> String {
    let resolved = options.resolve();
    // Include options in hash to invalidate cache when options change
    let options_str = format!(
        "show={:?},eval={},digits={:?}",
        resolved.show, resolved.eval, resolved.digits
    );
    let inline_content = format!("{}|{}|{}", code, options_str, previous_hash);

    let mut hasher = Sha256::new();
    hasher.update(inline_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Computes combined hash for file dependencies
///
/// Hashes the actual file content (SHA256) for each dependency.
/// Content hashing is more reliable than mtime-based approaches,
/// which can miss rapid changes on file systems with coarse-grained
/// timestamps (e.g. FAT32 has 2-second resolution).
///
/// Returns empty string if no dependencies
pub fn hash_dependencies(depends: &[PathBuf]) -> Result<String> {
    if depends.is_empty() {
        return Ok(String::new());
    }

    let mut outer_hasher = Sha256::new();

    for path in depends {
        if !path.exists() {
            return Err(anyhow!("Dependency not found: {:?}", path));
        }

        // Include path in outer hash to detect renames
        outer_hasher.update(path.to_string_lossy().as_bytes());

        // Hash the file content
        let content = fs::read(path)?;
        let mut file_hasher = Sha256::new();
        file_hasher.update(&content);
        outer_hasher.update(file_hasher.finalize());
    }

    Ok(format!("{:x}", outer_hasher.finalize()))
}
