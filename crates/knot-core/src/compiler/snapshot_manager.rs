//! Snapshot state follows the executed prefix of one language chain.
#![allow(missing_docs)]

use crate::cache::{Cache, SnapshotEntry, hashing::hash_file};
use crate::executors::KnotExecutor;
use anyhow::{Context, Result};

pub struct SnapshotManager {
    loaded_hash: Option<String>,
    allow_snapshots: bool,
    exec: Option<Box<dyn KnotExecutor>>,
    budget: super::snapshot_budget::SnapshotBudget,
    pub inline_warning: Option<String>,
    /// Whether the state before the next execution can be resumed from a
    /// snapshot; only the first non-reusable one in a chain is reported.
    resumable: bool,
}

impl SnapshotManager {
    pub fn new(exec: Option<Box<dyn KnotExecutor>>, allow_snapshots: bool) -> Self {
        Self {
            loaded_hash: None,
            allow_snapshots,
            exec,
            budget: Default::default(),
            inline_warning: None,
            resumable: true,
        }
    }

    pub fn warning(&mut self, node: &super::PlannedNode, cache: &Cache) -> Result<Option<String>> {
        if !self.allow_snapshots {
            return Ok(None);
        }
        self.budget.observe(
            cache,
            &node.hash,
            &node.lang,
            node.snapshot_warning_threshold,
        )
    }

    pub fn into_executor(self) -> Option<Box<dyn KnotExecutor>> {
        self.exec
    }

    pub fn executor_mut(&mut self) -> Option<&mut Box<dyn KnotExecutor>> {
        self.exec.as_mut()
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
        anyhow::ensure!(
            self.allow_snapshots,
            "Cannot restore a snapshot when snapshots are disabled"
        );
        let Some(exec) = self.exec.as_deref_mut() else {
            return Ok(());
        };
        cache
            .restore_snapshot(previous_hash, exec)
            .with_context(|| format!("Cannot restore {lang} state"))?;
        self.loaded_hash = Some(previous_hash.to_string());
        // Only reusable snapshots are restored.
        self.resumable = true;
        Ok(())
    }

    /// When enabled, replace the snapshot after actual execution, including cache:false
    /// and cache repairs. File existence alone does not identify interpreter state.
    ///
    /// The live session is at `hash` even if saving fails: the error only means
    /// that no snapshot is available, so the next compilation re-executes.
    ///
    /// Returns `true` when this snapshot is the first of the chain that cannot
    /// be reused: from here on, the chain replays at every compilation.
    pub fn record_execution(&mut self, lang: &str, hash: &str, cache: &mut Cache) -> Result<bool> {
        let result = self.save_snapshot(lang, hash, cache);
        if self.exec.is_some() {
            self.loaded_hash = Some(hash.to_string());
        }
        if result.is_err() {
            cache.metadata.snapshots.remove(hash);
        }
        let reusable = !self.allow_snapshots
            || cache
                .metadata
                .snapshots
                .get(hash)
                .is_some_and(|entry| entry.reusable);
        let first_non_reusable = result.is_ok() && self.resumable && !reusable;
        self.resumable = reusable;
        result.map(|()| first_non_reusable)
    }

    fn save_snapshot(&mut self, lang: &str, hash: &str, cache: &mut Cache) -> Result<()> {
        let Some(exec) = self.exec.as_deref_mut() else {
            return Ok(());
        };
        if !self.allow_snapshots {
            cache.metadata.snapshots.remove(hash);
            return Ok(());
        }
        let snapshot = cache.get_snapshot_path(hash, exec.snapshot_extension());
        exec.save_session(&snapshot)
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
                stats: Default::default(),
            },
        );
        // Just hashed: record the cheap identity used by later plans.
        cache.record_snapshot_stats(hash);
        Ok(())
    }
}
