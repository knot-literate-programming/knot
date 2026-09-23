//! Freeze contract: detection and registration of immutable cross-chunk objects.
#![allow(missing_docs)]

use crate::cache::{Cache, FreezeObjectInfo};
use crate::compiler::pipeline::PlannedNode;
use crate::executors::KnotExecutor;
use crate::executors::side_channel::RuntimeError;
use crate::parser::ast::Chunk;
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};

use super::pipeline::PlannedNodeKind;

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
    names: &[String],
    exec: &mut Box<dyn KnotExecutor>,
    cache: &Arc<Mutex<Cache>>,
) -> Result<()> {
    let chunk_name = chunk.label.as_deref().unwrap_or("unnamed").to_string();
    let cache_dir = cache.lock().unwrap().cache_dir.clone();

    for obj_name in names {
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
    exec: &mut Box<dyn KnotExecutor>,
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

    let chunk_name = match &pn.kind {
        PlannedNodeKind::Chunk { node, .. } => node.label.as_deref().unwrap_or("unnamed"),
        PlannedNodeKind::Inline { .. } => "inline",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TypstBackend;
    use crate::config::Config;
    use crate::executors::{
        ConstantObjectHandler, ExecutionAttempt, ExecutionOutput, ExecutionResult, GraphicsOptions,
        LanguageExecutor,
    };
    use crate::{Compiler, Document, Phase0Mode};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeExecutor {
        value: String,
        calls: Arc<AtomicUsize>,
        pause: Option<(
            std::sync::mpsc::Sender<()>,
            Mutex<std::sync::mpsc::Receiver<()>>,
        )>,
    }
    impl LanguageExecutor for FakeExecutor {
        fn initialize(&mut self) -> Result<()> {
            Ok(())
        }
        fn execute(&mut self, code: &str, _: &GraphicsOptions) -> Result<ExecutionAttempt> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some((started, release)) = self.pause.take() {
                started.send(()).unwrap();
                release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            }
            if code.trim() == "error" {
                return Ok(ExecutionAttempt::RuntimeError(RuntimeError {
                    message: Some("failure".into()),
                    call: None,
                    line: None,
                    traceback: vec![],
                }));
            }
            if code.trim() == "mutate" {
                self.value = "changed".into();
            }
            Ok(ExecutionAttempt::Success(ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
            }))
        }
        fn execute_inline(&mut self, _: &str) -> Result<String> {
            unreachable!()
        }
        fn query(&mut self, _: &str) -> Result<String> {
            unreachable!()
        }
    }
    impl KnotExecutor for FakeExecutor {
        fn save_session(&mut self, _: &Path) -> Result<()> {
            panic!("Frozen chains must not save snapshots")
        }
        fn load_session(&mut self, _: &Path) -> Result<()> {
            unreachable!()
        }
        fn snapshot_extension(&self) -> &'static str {
            "fake"
        }
    }
    impl ConstantObjectHandler for FakeExecutor {
        fn hash_object(&mut self, name: &str) -> Result<String> {
            assert_eq!(name, "x", "Only declared frozen names should be checked");
            Ok(self.value.clone())
        }
        fn save_constant(&mut self, _: &str, hash: &str, dir: &Path) -> Result<()> {
            std::fs::create_dir_all(dir.join("objects"))?;
            Ok(std::fs::write(
                dir.join("objects").join(format!("{hash}.fake")),
                &self.value,
            )?)
        }
        fn load_constant(&mut self, _: &str, _: &str, _: &Path) -> Result<()> {
            panic!("Frozen objects must not be reloaded after successful execution")
        }
        fn object_extension(&self) -> &'static str {
            "fake"
        }
    }

    #[test]
    fn declared_objects_pass_unchanged_and_fail_on_mutation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.knot");
        std::fs::write(&path, "").unwrap();
        let mut compiler = Compiler::new(&path).unwrap();
        let doc = Document::parse("```{python}\n#| freeze: [x]\nunchanged\n```".into());
        let (planned, cache, _) = compiler
            .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
            .unwrap();
        let mut exec: Box<dyn KnotExecutor> = Box::new(FakeExecutor {
            value: "original".into(),
            calls: Arc::default(),
            pause: None,
        });
        let chunk = &doc.chunks[0];
        register_freeze_objects(chunk, &chunk.options.freeze, &mut exec, &cache).unwrap();
        assert_eq!(cache.lock().unwrap().metadata.freeze_objects.len(), 1);
        // Reject even an otherwise intact legacy snapshot before loading anything.
        {
            let mut cache = cache.lock().unwrap();
            let file = cache.cache_dir.join("snapshot_legacy.fake");
            std::fs::write(&file, "state").unwrap();
            let frozen = cache.metadata.freeze_objects.clone();
            cache.metadata.snapshots.insert(
                "legacy".into(),
                crate::cache::SnapshotEntry {
                    reusable: true,
                    files: [(
                        "snapshot_legacy.fake".into(),
                        crate::cache::hashing::hash_file(&file).unwrap(),
                    )]
                    .into(),
                    freeze_objects: frozen,
                },
            );
            assert!(!cache.snapshot_is_valid("legacy"));
            assert!(cache.restore_snapshot("legacy", exec.as_mut()).is_err());
        }

        assert!(
            check_freeze_contract(&planned[0], &mut exec, &cache)
                .unwrap()
                .is_none()
        );
        let graphics = GraphicsOptions {
            width: 1.0,
            height: 1.0,
            dpi: 72,
            format: "svg".into(),
        };
        exec.execute("mutate", &graphics).unwrap();
        assert!(
            check_freeze_contract(&planned[0], &mut exec, &cache)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn freeze_and_runtime_errors_stop_the_language_chain_without_an_interpreter() {
        for failing_code in ["mutate", "error"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("main.knot");
            std::fs::write(&path, "").unwrap();
            let mut compiler = Compiler::new(&path).unwrap();
            let doc = Document::parse(format!(
                "```{{python}}\n#| freeze: [x]\nunchanged\n```\n```{{python}}\n{failing_code}\n```\n```{{python}}\nnever\n```"
            ));
            let (planned, cache, _) = compiler
                .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
                .unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let exec = Box::new(FakeExecutor {
                value: "original".into(),
                calls: Arc::clone(&calls),
                pause: None,
            });
            let (_, _, output) = crate::compiler::execution::run_language_chain(
                "python".into(),
                planned.into_iter().enumerate().collect(),
                Some(exec),
                cache,
                &TypstBackend::new(),
                &Config::default(),
                None,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert!(output[1].1.errored);
            assert!(!output[2].1.errored);
            assert!(output[2].1.typst_content.contains("inert"));
        }
    }
    #[test]
    fn cancellation_during_execution_stops_before_caching_or_the_next_chunk() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.knot");
        std::fs::write(&path, "").unwrap();
        let doc = Document::parse("```{python}\nfirst\n```\n```{python}\nnever\n```".into());
        let (planned, cache, _) = Compiler::new(&path)
            .unwrap()
            .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let cancellation = crate::cancellation::Cancellation::default();
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let exec = Box::new(FakeExecutor {
            value: "original".into(),
            calls: Arc::clone(&calls),
            pause: Some((started_tx, Mutex::new(released))),
        });
        let worker = std::thread::spawn({
            let cache = Arc::clone(&cache);
            let cancellation = cancellation.clone();
            move || {
                crate::compiler::execution::run_language_chain(
                    "python".into(),
                    planned.into_iter().enumerate().collect(),
                    Some(exec),
                    cache,
                    &TypstBackend::new(),
                    &Config::default(),
                    None,
                    &cancellation,
                )
            }
        });
        started
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        cancellation.cancel();
        release.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(cache.lock().unwrap().metadata.chunks.is_empty());
        assert!(cache.lock().unwrap().metadata.snapshots.is_empty());
    }
}
