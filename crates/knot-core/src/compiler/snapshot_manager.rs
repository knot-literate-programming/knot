//! Snapshot Management for Incremental Compilation
//!
//! Handles loading, saving, and restoring language runtime state (snapshots)
//! to enable incremental compilation and state persistence across chunks.

use crate::cache::Cache;
use crate::defaults::Defaults;
use crate::executors::ExecutorManager;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub struct SnapshotManager {
    /// Tracks the hash of the snapshot currently loaded in EACH executor
    loaded_snapshot_per_lang: HashMap<String, String>,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure the executor for the given language is in the state corresponding to `previous_hash`
    pub fn restore_if_needed(
        &mut self,
        lang: &str,
        previous_hash: &str,
        executor_manager: &mut ExecutorManager,
        cache: &Cache,
        project_root: &Path,
    ) -> Result<()> {
        if previous_hash.is_empty() {
            return Ok(());
        }

        let current_loaded = self
            .loaded_snapshot_per_lang
            .get(lang)
            .cloned()
            .unwrap_or_default();

        if current_loaded != previous_hash {
            let exec = executor_manager.get_executor(lang)?;
            let ext = exec.snapshot_extension();
            let snapshot_path = cache.get_snapshot_path(previous_hash, ext);

            if snapshot_path.exists() {
                log::debug!("Restoring state for {} from {}", lang, &previous_hash[..8]);

                // Load the session snapshot
                exec.load_session(&snapshot_path).context(format!(
                    "Failed to restore {} session snapshot for hash {}",
                    lang,
                    &previous_hash[..8]
                ))?;

                // Also restore constant objects for this language
                let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);
                for (obj_name, info) in &cache.metadata.constant_objects {
                    if info.language == lang {
                        exec.load_constant(obj_name, &info.hash, &cache_dir)
                            .context(format!(
                                "Failed to load constant '{}' into {} environment",
                                obj_name, lang
                            ))?;
                    }
                }

                self.loaded_snapshot_per_lang
                    .insert(lang.to_string(), previous_hash.to_string());
            }
        }

        Ok(())
    }

    /// Update the snapshot state after execution or cache hit
    pub fn update_after_node(
        &mut self,
        lang: &str,
        node_hash: &str,
        previous_hash: &str,
        executor_manager: &mut ExecutorManager,
        cache: &Cache,
        project_root: &Path,
    ) -> Result<()> {
        let exec = executor_manager.get_executor(lang)?;
        let ext = exec.snapshot_extension();
        let snapshot_path = cache.get_snapshot_path(node_hash, ext);
        let snapshot_exists = cache.has_snapshot(node_hash, ext);
        let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

        if !snapshot_exists {
            // CASE 1: Cache Miss (Executed)
            // The executor has just run the code. It is in state `node_hash`.

            // Temporarily remove constant objects to keep the snapshot lightweight
            for (obj_name, info) in &cache.metadata.constant_objects {
                if info.language == lang {
                    exec.remove_from_env(obj_name).context(format!(
                        "Failed to remove constant object '{}' from environment",
                        obj_name
                    ))?;
                }
            }

            // Save the session snapshot
            exec.save_session(&snapshot_path).context(format!(
                "Failed to save {} session snapshot for hash {}",
                lang,
                &node_hash[..8]
            ))?;

            // Restore constant objects to environment
            for (obj_name, info) in &cache.metadata.constant_objects {
                if info.language == lang {
                    exec.load_constant(obj_name, &info.hash, &cache_dir)
                        .context(format!(
                            "Failed to restore constant object '{}' to environment",
                            obj_name
                        ))?;
                }
            }

            log::debug!(
                "💾 Saved lightweight snapshot for node {} (lang: {})",
                &node_hash[..8],
                lang
            );

            // Update tracked state
            self.loaded_snapshot_per_lang
                .insert(lang.to_string(), node_hash.to_string());
        } else {
            // CASE 2: Cache Hit (Skipped)
            // The executor did NOT run the code. It is still in state `previous_hash`.
            // We do NOTHING here. loaded_snapshot_per_lang remains `previous_hash`.
            log::debug!(
                "⚡ Chunk {} cached. Executor stays at state {}.",
                &node_hash[..8],
                &previous_hash[..8]
            );
        }

        Ok(())
    }

    /// Mark a language state as potentially dirty or reset it
    pub fn reset_loaded_state(&mut self, lang: &str) {
        self.loaded_snapshot_per_lang.remove(lang);
    }
}
