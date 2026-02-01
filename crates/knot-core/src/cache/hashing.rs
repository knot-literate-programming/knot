// Cache Hashing Logic
//
// Handles SHA256-based hashing for:
// - Chunk code with options and dependencies (with chaining)
// - Inline expressions with verb
// - File dependencies (path + mtime + size)

use crate::parser::ChunkOptions;
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

/// Computes SHA256 hash for a chunk
///
/// Hash includes:
/// - Code content
/// - Serialized options (eval, echo, output, cache, graphics, etc.)
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
/// - Options (echo, eval, digits)
/// - Previous inline expression hash (for sequential invalidation)
pub fn get_inline_expr_hash(code: &str, options: &crate::parser::InlineOptions, previous_hash: &str) -> String {
    // Include options in hash to invalidate cache when options change
    let options_str = format!("echo={},eval={},digits={:?}",
        options.echo, options.eval, options.digits);
    let inline_content = format!("{}|{}|{}", code, options_str, previous_hash);

    let mut hasher = Sha256::new();
    hasher.update(inline_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Computes combined hash for file dependencies
///
/// Hash includes for each file:
/// - File path
/// - Last modified time
/// - File size
///
/// Returns empty string if no dependencies
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
