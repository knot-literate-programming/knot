//! Freeze contract: detection and registration of immutable cross-chunk objects.

use crate::cache::{Cache, FreezeObjectInfo};
use crate::compiler::pipeline::PlannedNode;
use crate::defaults::Defaults;
use crate::executors::KnotExecutor;
use crate::executors::side_channel::RuntimeError;
use crate::parser::ast::Chunk;
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::ExecutableNode;

/// Returns the composite cache key for a freeze object: `"lang::varname"`.
///
/// Using a composite key prevents name collisions when R and Python both
/// declare a freeze object with the same variable name.
fn freeze_key(lang: &str, name: &str) -> String {
    format!("{}::{}", lang, name)
}

/// Saves all freeze objects declared by a chunk to the object cache.
///
/// `exec` must be `Some` when this is called (freeze objects only arise from
/// successfully executed chunks, so an executor is always present).
pub(super) fn register_freeze_objects(
    chunk: &Chunk,
    exec: &mut Option<Box<dyn KnotExecutor>>,
    cache: &Arc<Mutex<Cache>>,
    project_root: &Path,
) -> Result<()> {
    let exec = exec
        .as_deref_mut()
        .expect("executor must be present when registering freeze objects");
    let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

    for obj_name in &chunk.options.freeze {
        let obj_hash = exec
            .hash_object(obj_name)
            .context(format!("Failed to hash freeze object '{}'", obj_name))?;

        exec.save_constant(obj_name, &obj_hash, &cache_dir)
            .context(format!("Failed to save freeze object '{}'", obj_name))?;

        let ext = exec.object_extension();
        let object_path = cache_dir
            .join("objects")
            .join(format!("{}.{}", obj_hash, ext));
        let size_bytes = std::fs::metadata(&object_path)?.len();

        let key = freeze_key(&chunk.language, obj_name);
        cache.lock().unwrap().metadata.freeze_objects.insert(
            key,
            FreezeObjectInfo {
                name: obj_name.clone(),
                hash: obj_hash,
                size_bytes,
                language: chunk.language.clone(),
                created_in_chunk: chunk_name.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        log::info!(
            "🔒 Freeze object '{}' ({}) declared in chunk '{}'",
            obj_name,
            chunk.language,
            chunk_name
        );
    }
    Ok(())
}

/// Checks that no freeze object for `pn`'s language was mutated during chunk execution.
///
/// Called after each successful MustExecute node, before saving the snapshot.
///
/// Returns:
/// - `Ok(None)` — all freeze contracts satisfied, execution can proceed
/// - `Ok(Some(error))` — a freeze object was mutated; `error` is a [`RuntimeError`]
///   whose `to_string()` gives a short PDF message and whose
///   `detailed_message()` gives the full LSP diagnostic
/// - `Err(e)` — the hash computation itself failed (propagated to the caller)
pub(super) fn check_freeze_contract(
    pn: &PlannedNode,
    exec: &mut Option<Box<dyn KnotExecutor>>,
    cache: &Arc<Mutex<Cache>>,
) -> Result<Option<RuntimeError>> {
    // Collect needed data while holding the lock briefly, then release before calling executor.
    let freeze_entries: Vec<FreezeObjectInfo> = {
        let cache_guard = cache.lock().unwrap();
        cache_guard
            .metadata
            .freeze_objects
            .values()
            .filter(|info| info.language == pn.lang)
            .cloned()
            .collect()
    };

    if freeze_entries.is_empty() {
        return Ok(None);
    }

    let exec = match exec.as_deref_mut() {
        Some(e) => e,
        None => return Ok(None),
    };

    let chunk_name = match &pn.node {
        ExecutableNode::Chunk(c) => c.name.as_deref().unwrap_or("unnamed"),
        ExecutableNode::InlineExpr(_) => "inline",
    };

    for info in &freeze_entries {
        let current_hash = exec
            .hash_object(&info.name)
            .context(format!("Failed to hash freeze object '{}'", info.name))?;

        if current_hash != info.hash {
            let error = RuntimeError {
                message: Some(format!(
                    "Freeze contract violated: object '{}' ({}) was modified in chunk '{}'",
                    info.name, info.language, chunk_name
                )),
                call: None,
                line: None,
                traceback: vec![
                    format!(
                        "Object '{}' was frozen in chunk '{}'",
                        info.name, info.created_in_chunk
                    ),
                    format!("Expected hash : {}", info.hash),
                    format!("Current hash  : {}", current_hash),
                    String::from("Frozen objects must not be mutated after declaration."),
                ],
            };
            return Ok(Some(error));
        }
    }

    Ok(None)
}
