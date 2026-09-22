//! SHA-256 hashing for chunks, inline expressions and file dependencies.
//!
//! Chunk hashes are chained: each hash includes the previous chunk's hash so
//! that editing chunk N automatically invalidates N+1, N+2, … in the same
//! language chain.  File dependencies are hashed by content so that changing
//! an external file is detected immediately, without relying on mtime.

use crate::parser::ChunkOptions;
use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

/// Hash an execution using unambiguous, length-prefixed fields.
/// The version namespaces both the identity rules and the snapshot format.
fn hash_fields(fields: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
}

/// Computes a chunk's execution identity. Presentation options are deliberately
/// excluded: cached raw results are rendered again with the current options.
pub fn get_chunk_hash(
    language: &str,
    code: &str,
    options: &ChunkOptions,
    previous_hash: &str,
    dependencies_hash: &str,
) -> String {
    chunk_hash_with_version(
        language,
        code,
        options,
        previous_hash,
        dependencies_hash,
        &crate::SCRIPTS_VERSION,
    )
}

fn chunk_hash_with_version(
    language: &str,
    code: &str,
    options: &ChunkOptions,
    previous_hash: &str,
    dependencies_hash: &str,
    scripts_version: &str,
) -> String {
    let resolved = options.resolve();
    let execution_options = format!(
        "{}|{}|{}|{}|{:?}",
        resolved.eval, resolved.fig_width, resolved.fig_height, resolved.dpi, resolved.fig_format
    );
    let freeze = hash_fields(
        &options
            .freeze
            .iter()
            .map(|name| name.as_bytes())
            .collect::<Vec<_>>(),
    );
    hash_fields(&[
        b"knot-cache-v2",
        b"chunk",
        language.as_bytes(),
        code.as_bytes(),
        execution_options.as_bytes(),
        freeze.as_bytes(),
        previous_hash.as_bytes(),
        dependencies_hash.as_bytes(),
        scripts_version.as_bytes(),
    ])
}

/// Computes an inline expression identity, including its language and rendering
/// options because inline cache entries contain the formatted text.
pub fn get_inline_expr_hash(
    language: &str,
    code: &str,
    options: &crate::parser::InlineOptions,
    previous_hash: &str,
) -> String {
    let resolved = options.resolve();
    let options = format!(
        "{:?}|{}|{:?}",
        resolved.show, resolved.eval, resolved.digits
    );
    hash_fields(&[
        b"knot-cache-v2",
        b"inline",
        language.as_bytes(),
        code.as_bytes(),
        options.as_bytes(),
        previous_hash.as_bytes(),
        crate::SCRIPTS_VERSION.as_bytes(),
    ])
}

/// Fingerprint a file's actual contents, for cache integrity checks.
pub fn hash_file(path: &std::path::Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{InlineOptions, Show};

    #[test]
    fn identity_includes_language_scripts_graphics_freeze_and_dependencies() {
        let base = ChunkOptions::default();
        let hash = |lang: &str, options: &ChunkOptions, prev: &str, deps: &str, version: &str| {
            chunk_hash_with_version(lang, "print(1)", options, prev, deps, version)
        };
        let original = hash("r", &base, "", "", "v1");
        assert_ne!(original, hash("python", &base, "", "", "v1"));
        assert_ne!(original, hash("r", &base, "", "", "v2"));
        assert_ne!(original, hash("r", &base, "prefix", "", "v1"));
        assert_ne!(original, hash("r", &base, "", "data", "v1"));
        let mut changed = base.clone();
        changed.fig_width = Some(99.0);
        assert_ne!(original, hash("r", &changed, "", "", "v1"));
        changed = base.clone();
        changed.freeze = vec!["x".into()];
        assert_ne!(original, hash("r", &changed, "", "", "v1"));
        changed = base;
        changed.show = Some(Show::None);
        changed.cache = Some(false);
        assert_eq!(original, hash("r", &changed, "", "", "v1"));
    }

    #[test]
    fn field_boundaries_and_inline_language_cannot_collide() {
        assert_ne!(hash_fields(&[b"a", b"bc"]), hash_fields(&[b"ab", b"c"]));
        assert_ne!(
            get_inline_expr_hash("r", "1", &InlineOptions::default(), ""),
            get_inline_expr_hash("python", "1", &InlineOptions::default(), "")
        );
    }
}
