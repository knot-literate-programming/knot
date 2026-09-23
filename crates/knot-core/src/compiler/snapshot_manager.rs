//! Snapshot state follows the executed prefix of one language chain.
#![allow(missing_docs)]

use crate::cache::{Cache, SnapshotEntry, hashing::hash_file};
use crate::executors::KnotExecutor;
use anyhow::{Context, Result};
use std::collections::HashMap;

pub struct SnapshotManager {
    loaded_hash: Option<String>,
    exec: Option<Box<dyn KnotExecutor>>,
}

impl SnapshotManager {
    pub fn new(exec: Option<Box<dyn KnotExecutor>>) -> Self {
        Self {
            loaded_hash: None,
            exec,
        }
    }

    pub fn into_executor(self) -> Option<Box<dyn KnotExecutor>> {
        self.exec
    }

    pub fn executor_mut(&mut self) -> Option<&mut Box<dyn KnotExecutor>> {
        self.exec.as_mut()
    }

    /// Activate the freeze declarations from this prefix, without starting an
    /// interpreter or pretending that cached code has just executed.
    pub fn activate_cached(lang: &str, hash: &str, cache: &mut Cache) -> Result<()> {
        let frozen = cache
            .metadata
            .snapshots
            .get(hash)
            .context("Missing snapshot metadata for cached execution")?
            .freeze_objects
            .clone();
        cache
            .metadata
            .freeze_objects
            .retain(|_, info| info.language != lang);
        cache.metadata.freeze_objects.extend(frozen);
        Ok(())
    }

    pub fn restore_if_needed(
        &mut self,
        lang: &str,
        previous_hash: &str,
        cache: &Cache,
    ) -> Result<()> {
        if previous_hash.is_empty() || self.loaded_hash.as_deref() == Some(previous_hash) {
            return Ok(());
        }
        let Some(exec) = self.exec.as_deref_mut() else {
            return Ok(());
        };
        cache
            .restore_snapshot(previous_hash, exec)
            .with_context(|| format!("Cannot restore {lang} state"))?;
        self.loaded_hash = Some(previous_hash.to_string());
        Ok(())
    }

    /// Always replace the snapshot after actual execution, including cache:false
    /// and cache repairs. File existence alone does not identify interpreter state.
    pub fn record_execution(&mut self, lang: &str, hash: &str, cache: &mut Cache) -> Result<()> {
        let Some(exec) = self.exec.as_deref_mut() else {
            return Ok(());
        };
        let frozen: HashMap<_, _> = cache
            .metadata
            .freeze_objects
            .iter()
            .filter(|(_, info)| info.language == lang)
            .map(|(key, info)| (key.clone(), info.clone()))
            .collect();
        let excluded = frozen
            .values()
            .map(|info| info.name.clone())
            .collect::<Vec<_>>();
        let snapshot = cache.get_snapshot_path(hash, exec.snapshot_extension());
        exec.save_session_excluding(&snapshot, &excluded)
            .with_context(|| format!("Failed to save {lang} snapshot {}", snapshot.display()))?;

        let reusable = !snapshot.with_extension("replay").exists();
        let mut paths = vec![snapshot];
        if exec.snapshot_extension() == "RData" {
            paths.push(
                cache
                    .cache_dir
                    .join(format!("snapshot_{hash}_packages.rds")),
            );
        }
        for info in frozen.values() {
            paths.push(cache.cache_dir.join("objects").join(format!(
                "{}.{}",
                info.hash,
                exec.object_extension()
            )));
        }
        let files = paths
            .into_iter()
            .map(|path| {
                let relative = path
                    .strip_prefix(&cache.cache_dir)?
                    .to_string_lossy()
                    .into_owned();
                Ok((relative, hash_file(&path)?))
            })
            .collect::<Result<_>>()?;
        cache.metadata.snapshots.insert(
            hash.to_string(),
            SnapshotEntry {
                reusable,
                files,
                freeze_objects: frozen,
            },
        );
        self.loaded_hash = Some(hash.to_string());
        Ok(())
    }
}
