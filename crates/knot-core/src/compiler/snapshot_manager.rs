//! Snapshot Management for Incremental Compilation
//!
//! Handles loading, saving, and restoring language runtime state (snapshots)
//! to enable incremental compilation and state persistence across chunks.
//!
//! The `SnapshotManager` owns the executor for its language chain, so that
//! snapshot operations (`restore_if_needed`, `update_after_node`) can access
//! the executor directly without requiring it to be threaded through every
//! call site as a `&mut Option<Box<dyn KnotExecutor>>`.

use crate::cache::Cache;
use crate::defaults::Defaults;
use crate::executors::KnotExecutor;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Returns up to the first 8 characters of a hash string, safe for any length.
fn short_hash(h: &str) -> &str {
    &h[..h.len().min(8)]
}

pub struct SnapshotManager {
    /// Tracks the hash of the snapshot currently loaded in the executor.
    loaded_snapshot_per_lang: HashMap<String, String>,
    /// The executor owned by this manager (one per language chain).
    exec: Option<Box<dyn KnotExecutor>>,
}

impl SnapshotManager {
    pub fn new(exec: Option<Box<dyn KnotExecutor>>) -> Self {
        Self {
            loaded_snapshot_per_lang: HashMap::new(),
            exec,
        }
    }

    /// Consume the manager and return the executor back to the caller.
    ///
    /// Called at the end of a language chain so the executor can be returned
    /// to the `ExecutorManager`.
    pub fn into_executor(self) -> Option<Box<dyn KnotExecutor>> {
        self.exec
    }

    /// Returns a mutable reference to the executor box, or `None` if the
    /// language is not supported.  Callers that require an executor handle the
    /// `None` case once; method calls on the box are auto-dereffed through
    /// `Box<dyn KnotExecutor>` transparently.
    pub fn executor_mut(&mut self) -> Option<&mut Box<dyn KnotExecutor>> {
        self.exec.as_mut()
    }

    /// Ensure the executor is in the state corresponding to `previous_hash`.
    ///
    /// If the executor is `None` (language not started), returns `Ok(())`
    /// immediately.
    pub fn restore_if_needed(
        &mut self,
        lang: &str,
        previous_hash: &str,
        cache: &Cache,
        project_root: &Path,
    ) -> Result<()> {
        let exec = match self.exec.as_deref_mut() {
            Some(e) => e,
            None => return Ok(()),
        };

        if previous_hash.is_empty() {
            return Ok(());
        }

        let current_loaded = self
            .loaded_snapshot_per_lang
            .get(lang)
            .cloned()
            .unwrap_or_default();

        if current_loaded != previous_hash {
            let ext = exec.snapshot_extension();
            let snapshot_path = cache.get_snapshot_path(previous_hash, ext);

            if snapshot_path.exists() {
                log::debug!(
                    "Restoring state for {} from {}",
                    lang,
                    if previous_hash.is_empty() {
                        "N/A"
                    } else {
                        short_hash(previous_hash)
                    }
                );

                exec.load_session(&snapshot_path).context(format!(
                    "Failed to restore {} session snapshot for hash {}",
                    lang,
                    short_hash(previous_hash)
                ))?;

                // Also restore constant objects for this language.
                let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);
                for info in cache.metadata.freeze_objects.values() {
                    if info.language == lang {
                        exec.load_constant(&info.name, &info.hash, &cache_dir)
                            .context(format!(
                                "Failed to load constant '{}' into {} environment",
                                info.name, lang
                            ))?;
                    }
                }

                self.loaded_snapshot_per_lang
                    .insert(lang.to_string(), previous_hash.to_string());
            }
        }

        Ok(())
    }

    /// Update the snapshot state after execution or cache hit.
    ///
    /// If the executor is `None`, returns `Ok(())` immediately — for
    /// cache-hit-only chains the snapshot already exists on disk and no
    /// executor operations are needed.
    pub fn update_after_node(
        &mut self,
        lang: &str,
        node_hash: &str,
        previous_hash: &str,
        cache: &Cache,
        project_root: &Path,
    ) -> Result<()> {
        let exec = match self.exec.as_deref_mut() {
            Some(e) => e,
            None => return Ok(()),
        };

        let ext = exec.snapshot_extension();
        let snapshot_path = cache.get_snapshot_path(node_hash, ext);
        let snapshot_exists = cache.has_snapshot(node_hash, ext);
        let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

        if !snapshot_exists {
            // CASE 1: Cache Miss (Executed)
            // The executor has just run the code. It is in state `node_hash`.

            // Temporarily remove constant objects to keep the snapshot lightweight.
            for info in cache.metadata.freeze_objects.values() {
                if info.language == lang {
                    exec.remove_from_env(&info.name).context(format!(
                        "Failed to remove constant object '{}' from environment",
                        info.name
                    ))?;
                }
            }

            exec.save_session(&snapshot_path).context(format!(
                "Failed to save {} session snapshot for hash {}",
                lang,
                short_hash(node_hash)
            ))?;

            // Restore constant objects to environment.
            for info in cache.metadata.freeze_objects.values() {
                if info.language == lang {
                    exec.load_constant(&info.name, &info.hash, &cache_dir)
                        .context(format!(
                            "Failed to restore constant object '{}' to environment",
                            info.name
                        ))?;
                }
            }

            log::debug!(
                "💾 Saved lightweight snapshot for node {} (lang: {})",
                if node_hash.is_empty() {
                    "N/A"
                } else {
                    short_hash(node_hash)
                },
                lang
            );

            self.loaded_snapshot_per_lang
                .insert(lang.to_string(), node_hash.to_string());
        } else {
            // CASE 2: Cache Hit (Skipped)
            // The executor did NOT run the code. It is still in state `previous_hash`.
            // We do NOTHING here. loaded_snapshot_per_lang remains `previous_hash`.
            log::debug!(
                "⚡ Chunk {} cached. Executor stays at state {}.",
                if node_hash.is_empty() {
                    "N/A"
                } else {
                    short_hash(node_hash)
                },
                if previous_hash.is_empty() {
                    "N/A"
                } else {
                    short_hash(previous_hash)
                }
            );
        }

        Ok(())
    }

    /// Mark a language state as potentially dirty or reset it.
    pub fn reset_loaded_state(&mut self, lang: &str) {
        self.loaded_snapshot_per_lang.remove(lang);
    }
}
