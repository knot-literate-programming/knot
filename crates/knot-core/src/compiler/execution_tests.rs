use super::*;
use crate::backend::TypstBackend;
use crate::config::Config;
use crate::executors::KnotExecutor;
use crate::executors::RuntimeError;
use crate::executors::{
    ExecutionAttempt, ExecutionOutput, ExecutionResult, GraphicsOptions, LanguageExecutor,
};
use crate::{Compiler, Document, Phase0Mode};
use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FakeExecutor {
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
        Ok(ExecutionAttempt::Success(ExecutionOutput {
            result: ExecutionResult::Text(String::new()),
            exports: Vec::new(),
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
        panic!("Disabled snapshots must not be saved")
    }
    fn load_session(&mut self, _: &Path) -> Result<()> {
        unreachable!()
    }
    fn snapshot_extension(&self) -> &'static str {
        "fake"
    }
}
#[test]
fn runtime_errors_stop_the_language_chain_without_an_interpreter() {
    let failing_code = "error";
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    std::fs::write(&path, "").unwrap();
    let mut compiler = Compiler::new(&path).unwrap();
    let doc = Document::parse(format!(
        "---\nsnapshots:\n  python: false\n---\n```{{python}}\nunchanged\n```\n```{{python}}\n{failing_code}\n```\n```{{python}}\nnever\n```"
    ));
    let (planned, cache, _) = compiler
        .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let exec = Box::new(FakeExecutor {
        calls: Arc::clone(&calls),
        pause: None,
    });
    let (_, _, output) = crate::compiler::execution::run_language_chain(
        "python".into(),
        planned.into_iter().enumerate().collect(),
        Some(exec),
        None,
        cache,
        &TypstBackend::new(),
        &Config::default(),
        None,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(output[1].1.error.is_some());
    assert!(output[2].1.error.is_none());
    assert!(output[2].1.typst_content.contains("inert"));
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
                None,
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

/// Saves or restores sessions on demand, failing when asked to.
struct SessionExecutor {
    calls: Arc<AtomicUsize>,
    fail_save: bool,
    fail_load: bool,
    /// From this execution count on, saved sessions need replay (like a
    /// Python session holding an object that cannot be pickled).
    non_reusable_from: Option<usize>,
}
impl LanguageExecutor for SessionExecutor {
    fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    fn execute(&mut self, _: &str, _: &GraphicsOptions) -> Result<ExecutionAttempt> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ExecutionAttempt::Success(ExecutionOutput {
            result: ExecutionResult::Text("done".into()),
            exports: Vec::new(),
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
impl KnotExecutor for SessionExecutor {
    fn save_session(&mut self, path: &Path) -> Result<()> {
        anyhow::ensure!(!self.fail_save, "disk full");
        std::fs::write(path, "session")?;
        let replay = path.with_extension("replay");
        if self
            .non_reusable_from
            .is_some_and(|from| self.calls.load(Ordering::SeqCst) >= from)
        {
            std::fs::write(replay, "replay")?;
        } else if replay.exists() {
            std::fs::remove_file(replay)?;
        }
        Ok(())
    }
    fn load_session(&mut self, _: &Path) -> Result<()> {
        anyhow::ensure!(!self.fail_load, "incompatible snapshot");
        Ok(())
    }
    fn snapshot_extension(&self) -> &'static str {
        "fake"
    }
}

fn run_with(
    compiler: &mut Compiler,
    source: &str,
    exec: SessionExecutor,
) -> Vec<(usize, crate::compiler::ExecutedNode)> {
    let doc = Document::parse(source.into());
    let (planned, cache, _) = compiler
        .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
        .unwrap();
    let (_, _, output) = crate::compiler::execution::run_language_chain(
        "python".into(),
        planned.into_iter().enumerate().collect(),
        Some(Box::new(exec)),
        None,
        Arc::clone(&cache),
        &TypstBackend::new(),
        &Config::default(),
        None,
        &Default::default(),
    )
    .unwrap();
    cache.lock().unwrap().save_metadata().unwrap();
    output
}

#[test]
fn failing_snapshot_save_is_a_warning_and_the_chain_continues() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    std::fs::write(&path, "").unwrap();
    let mut compiler = Compiler::new(&path).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let output = run_with(
        &mut compiler,
        "```{python}\nfirst\n```\n```{python}\nsecond\n```",
        SessionExecutor {
            calls: Arc::clone(&calls),
            fail_save: true,
            fail_load: true,
            non_reusable_from: None,
        },
    );
    // The live session is still usable: no restore, both chunks run.
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(output.iter().all(|(_, node)| node.error.is_none()));
    let first = &output[0].1.typst_content;
    assert!(first.contains("warnings: ("), "{first}");
    assert!(
        first.contains("Could not save the python session snapshot") && first.contains("disk full"),
        "{first}"
    );
}

#[test]
fn failing_snapshot_restore_is_rendered_and_suspends_the_chain() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    std::fs::write(&path, "").unwrap();
    let mut compiler = Compiler::new(&path).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let session = |fail_load| SessionExecutor {
        calls: Arc::clone(&calls),
        fail_save: false,
        fail_load,
        non_reusable_from: None,
    };
    run_with(
        &mut compiler,
        "```{python}\nfirst\n```\n```{python}\nsecond\n```",
        session(false),
    );
    // Editing the second chunk requires restoring the first chunk's snapshot.
    let output = run_with(
        &mut compiler,
        "```{python}\nfirst\n```\n```{python}\nedited\n```\n```{python}\nthird\n```",
        session(true),
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "nothing runs after the failure"
    );
    assert!(output[1].1.error.is_some());
    assert!(
        output[1].1.typst_content.contains("incompatible snapshot"),
        "{}",
        output[1].1.typst_content
    );
    assert!(output[1].1.typst_content.contains("re-executes this chain"));
    assert!(output[1].1.typst_content.contains("```python\nedited"));
    assert!(output[2].1.typst_content.contains("is-inert: true"));
}

#[test]
fn only_the_first_non_reusable_snapshot_of_a_chain_is_reported() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    std::fs::write(&path, "").unwrap();
    let message = crate::defaults::non_reusable_snapshot_message("python");
    let chain = "```{python}\nfirst\n```\n```{python}\nsecond\n```\n```{python}\nthird\n```";
    let warned = |source: &str, non_reusable_from| {
        let mut compiler = Compiler::new(&path).unwrap();
        run_with(
            &mut compiler,
            source,
            SessionExecutor {
                calls: Arc::new(AtomicUsize::new(0)),
                fail_save: false,
                fail_load: false,
                non_reusable_from,
            },
        )
        .iter()
        .map(|(_, node)| node.typst_content.contains(&message[..40]))
        .collect::<Vec<_>>()
    };
    // The second chunk makes the session non-reusable; the third inherits it.
    assert_eq!(warned(chain, Some(2)), [false, true, false]);
    // A serializable session produces no warning.
    assert_eq!(warned(chain, None), [false, false, false]);
    // Without snapshots nothing is saved, so nothing is reported.
    let disabled = format!("---\nsnapshots: {{python: false}}\n---\n{chain}");
    assert_eq!(warned(&disabled, Some(1)), [false, false, false]);
}
