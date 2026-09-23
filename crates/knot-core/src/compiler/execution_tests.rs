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
