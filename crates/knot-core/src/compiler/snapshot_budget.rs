//! Account only for snapshots encountered in the current language chain.
use crate::cache::Cache;
use anyhow::{Context, Result};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct SnapshotBudget {
    seen: HashSet<String>,
    bytes: u64,
    warned: bool,
}

impl SnapshotBudget {
    pub fn observe(
        &mut self,
        cache: &Cache,
        hash: &str,
        language: &str,
        threshold: Option<u64>,
    ) -> Result<Option<String>> {
        let Some(limit) = threshold else {
            return Ok(None);
        };
        if self.warned {
            return Ok(None);
        }
        if let Some(snapshot) = cache.metadata.snapshots.get(hash) {
            for path in snapshot.files.keys() {
                if self.seen.insert(path.clone()) {
                    let size = std::fs::metadata(cache.cache_dir.join(path))
                        .with_context(|| format!("Cannot measure snapshot {path}"))?
                        .len();
                    self.bytes = self.bytes.saturating_add(size);
                }
            }
        }
        if self.bytes <= limit {
            return Ok(None);
        }
        self.warned = true;
        Ok(Some(format!(
            "Knot: {language} snapshots in this file occupy {} (limit: {}). Consider disabling snapshots for this language or splitting the analysis into separate files. Disabling snapshots re-executes the entire language chain on every compilation.",
            display_bytes(self.bytes),
            display_bytes(limit)
        )))
    }
}

fn display_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB ({bytes} bytes)", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB ({bytes} bytes)", bytes as f64 / 1_000_000.0)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::SnapshotEntry;

    #[test]
    fn counts_current_files_once_and_warns_only_above_threshold() {
        let root = tempfile::tempdir().unwrap();
        let mut cache = Cache::new(root.path().to_owned()).unwrap();
        for (hash, file) in [("a", "a.pkl"), ("b", "b.pkl"), ("obsolete", "old.pkl")] {
            std::fs::write(root.path().join(file), b"12345").unwrap();
            cache.metadata.snapshots.insert(
                hash.into(),
                SnapshotEntry {
                    reusable: true,
                    files: [(file.into(), "unused".into())].into(),
                    stats: Default::default(),
                },
            );
        }
        let mut budget = SnapshotBudget::default();
        assert!(
            budget
                .observe(&cache, "a", "python", Some(5))
                .unwrap()
                .is_none()
        );
        assert!(
            budget
                .observe(&cache, "a", "python", Some(5))
                .unwrap()
                .is_none()
        );
        assert!(
            budget
                .observe(&cache, "b", "python", Some(5))
                .unwrap()
                .unwrap()
                .contains("10 bytes")
        );
        assert!(
            budget
                .observe(&cache, "b", "python", Some(5))
                .unwrap()
                .is_none()
        );
        assert!(
            SnapshotBudget::default()
                .observe(&cache, "b", "python", None)
                .unwrap()
                .is_none()
        );
    }
}
