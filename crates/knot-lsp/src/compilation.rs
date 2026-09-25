//! Per-project ordering: requests and publication share one gate.
use crate::KnotLanguageServer;
use knot_core::{Phase0Mode, ProjectOutput, cancellation::Cancellation, project::ProjectBuild};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{Mutex, MutexGuard};
use tower_lsp::lsp_types::Url;

#[derive(Default)]
pub(crate) struct Projects(Mutex<HashMap<PathBuf, Arc<Project>>>);
#[derive(Default)]
pub(crate) struct Project {
    pub gate: Mutex<Generation>,
    pub preview_start: Mutex<()>,
}
#[derive(Default)]
pub(crate) struct Generation {
    id: u64,
    cancellation: Cancellation,
}
#[derive(Clone)]
pub(crate) struct Request {
    pub project: Arc<Project>,
    pub root: PathBuf,
    pub id: u64,
    pub cancellation: Cancellation,
}
impl Projects {
    pub async fn project(&self, root: PathBuf) -> Arc<Project> {
        Arc::clone(self.0.lock().await.entry(root).or_default())
    }
    pub async fn cancel_all(&self) {
        let projects: Vec<_> = self.0.lock().await.values().cloned().collect();
        for project in projects {
            project.gate.lock().await.cancellation.cancel();
        }
    }
    pub async fn begin(&self, root: PathBuf) -> Request {
        let project = self.project(root.clone()).await;
        let mut generation = project.gate.lock().await;
        generation.cancellation.cancel();
        generation.id += 1;
        generation.cancellation = Cancellation::default();
        Request {
            project: Arc::clone(&project),
            root,
            id: generation.id,
            cancellation: generation.cancellation.clone(),
        }
    }
}
impl Request {
    pub async fn publication(&self) -> Option<MutexGuard<'_, Generation>> {
        let guard = self.current().await?;
        if self.cancellation.is_cancelled() {
            None
        } else {
            Some(guard)
        }
    }
    async fn current(&self) -> Option<MutexGuard<'_, Generation>> {
        let guard = self.project.gate.lock().await;
        if guard.id == self.id {
            Some(guard)
        } else {
            None
        }
    }
    fn params(&self, uri: &Url, versions: &HashMap<String, i32>) -> serde_json::Value {
        serde_json::json!({"uri":uri,"project":self.root,"generation":self.id,"versions":versions})
    }
}

impl KnotLanguageServer {
    pub(crate) async fn open_buffers(
        &self,
        root: &Path,
    ) -> (HashMap<PathBuf, String>, HashMap<String, i32>) {
        let documents = self.state.documents.read().await;
        let docs: HashMap<_, _> = documents
            .iter()
            .filter(|(uri, _)| {
                uri.to_file_path()
                    .ok()
                    .and_then(|path| knot_core::Config::find_project_root(&path).ok())
                    .and_then(|path| path.canonicalize().ok())
                    .as_deref()
                    == Some(root)
            })
            .collect();
        let buffers = docs
            .iter()
            .filter_map(|(uri, doc)| uri.to_file_path().ok().map(|p| (p, doc.text.clone())))
            .collect();
        let versions = docs
            .iter()
            .map(|(uri, doc)| (uri.to_string(), doc.version))
            .collect();
        (buffers, versions)
    }

    /// Register now (including typing); debounce delays work, never invalidation.
    pub(crate) async fn queue_compile(&self, uri: &Url, full: bool, debounce: bool) {
        let Ok(path) = uri.to_file_path() else { return };
        let Ok(root) =
            knot_core::Config::find_project_root(&path).and_then(|p| Ok(p.canonicalize()?))
        else {
            return;
        };
        let request = self.state.compilations.begin(root).await;
        let Some(guard) = request.publication().await else {
            return;
        };
        let (buffers, versions) = self.open_buffers(&request.root).await;
        if full {
            self.client
                .send_notification::<crate::KnotCompilationStarted>(request.params(uri, &versions))
                .await;
        } else {
            self.client
                .send_notification::<crate::KnotCompilationInvalidated>(
                    request.params(uri, &versions),
                )
                .await;
        }
        drop(guard);
        let this = self.clone_for_task();
        let uri = uri.clone();
        tokio::spawn(async move {
            if debounce {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            if request.cancellation.is_cancelled() {
                return;
            }
            this.run_compilation(uri, buffers, versions, request, full)
                .await;
        });
    }

    async fn run_compilation(
        &self,
        uri: Url,
        buffers: HashMap<PathBuf, String>,
        versions: HashMap<String, i32>,
        request: Request,
        full: bool,
    ) {
        if !full {
            self.run_preview(uri, buffers, versions, request).await;
            return;
        }
        // Copy committed cache under the same gate used by publishers, then release
        // it before any interpreter work. Different projects never share a gate.
        let Some(guard) = request.publication().await else {
            return;
        };
        let cancel = request.cancellation.clone();
        let path = request.root.clone();
        let prepared =
            tokio::task::spawn_blocking(move || ProjectBuild::prepare(&path, &buffers, cancel))
                .await
                .map_err(anyhow::Error::from)
                .and_then(std::convert::identity);
        drop(guard);
        let build = match prepared {
            Ok(build) => Arc::new(build),
            Err(error) => {
                self.finish_compile(&uri, &versions, &request, false, full)
                    .await;
                log::warn!("Project preparation failed: {error}");
                return;
            }
        };
        let phase0 = tokio::task::spawn_blocking({
            let build = Arc::clone(&build);
            move || {
                build.phase0(if full {
                    Phase0Mode::Pending
                } else {
                    Phase0Mode::Modified
                })
            }
        })
        .await;
        match phase0 {
            Ok(Ok(output)) => {
                if !self.publish_build(&request, &build, output, false).await {
                    self.finish_compile(&uri, &versions, &request, false, full)
                        .await;
                    return;
                }
            }
            _ => {
                self.finish_compile(&uri, &versions, &request, false, full)
                    .await;
                return;
            }
        }
        if !full {
            return;
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let worker = tokio::task::spawn_blocking({
            let build = Arc::clone(&build);
            move || {
                build.compile(Some(Box::new(move |output| {
                    tx.send(output)
                        .map_err(|_| anyhow::anyhow!("Publication receiver closed"))
                })))
            }
        });
        while let Some(output) = rx.recv().await {
            // Continue draining after cancellation so the blocking worker is joined.
            self.publish_build(&request, &build, output, false).await;
        }
        let result = worker
            .await
            .map_err(anyhow::Error::from)
            .and_then(std::convert::identity);
        match result {
            Ok(output) => {
                let Some(_guard) = request.publication().await else {
                    return;
                };
                let published = async {
                    build.publish(&output, true)?;
                    self.send_overlay(&output.typ_content, &output.main_typ_path)
                        .await
                }
                .await;
                if published.is_ok() {
                    for path in build.source_paths() {
                        if let Ok(source_uri) = Url::from_file_path(path) {
                            self.refresh_runtime_diagnostics(&source_uri, &versions)
                                .await;
                            self.sync_with_cache(&source_uri).await;
                        }
                    }
                } else if let Err(error) = &published {
                    log::warn!("Publication failed: {error}");
                }
                let mut params = request.params(&uri, &versions);
                params["success"] = serde_json::json!(published.is_ok());
                self.client
                    .send_notification::<crate::KnotCompilationComplete>(params)
                    .await;
            }
            Err(error) => {
                if !request.cancellation.is_cancelled() {
                    log::warn!("Compilation failed: {error}");
                }
                self.finish_compile(&uri, &versions, &request, false, true)
                    .await;
            }
        }
    }

    /// Typing: Phase 0 from the committed caches, without copying them, so
    /// that its cost does not depend on their size. Preparation and rendering
    /// both run under the publication gate, so no publisher changes the caches
    /// while they are read; nothing is executed.
    async fn run_preview(
        &self,
        uri: Url,
        buffers: HashMap<PathBuf, String>,
        versions: HashMap<String, i32>,
        request: Request,
    ) {
        let Some(guard) = request.publication().await else {
            return;
        };
        let cancel = request.cancellation.clone();
        let path = request.root.clone();
        let rendered = tokio::task::spawn_blocking(move || {
            let build = ProjectBuild::prepare_preview(&path, &buffers, cancel)?;
            let output = build.phase0(Phase0Mode::Modified)?;
            Ok::<_, anyhow::Error>((build, output))
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(std::convert::identity);
        drop(guard);
        match rendered {
            Ok((build, output)) => {
                self.publish_build(&request, &build, output, false).await;
            }
            Err(error) => {
                log::warn!("Preview failed: {error}");
                self.finish_compile(&uri, &versions, &request, false, false)
                    .await;
            }
        }
    }

    async fn publish_build(
        &self,
        request: &Request,
        build: &ProjectBuild,
        output: ProjectOutput,
        complete: bool,
    ) -> bool {
        let Some(_guard) = request.publication().await else {
            return false;
        };
        if let Err(error) = build.publish(&output, complete) {
            log::warn!("Publication failed: {error}");
            request.cancellation.cancel();
            return false;
        }
        if let Err(error) = self
            .send_overlay(&output.typ_content, &output.main_typ_path)
            .await
        {
            log::warn!("Overlay publication failed: {error}");
            request.cancellation.cancel();
            return false;
        }
        true
    }
    async fn finish_compile(
        &self,
        uri: &Url,
        versions: &HashMap<String, i32>,
        request: &Request,
        success: bool,
        full: bool,
    ) {
        if !full {
            return;
        }
        let Some(_guard) = request.current().await else {
            return;
        };
        let mut params = request.params(uri, versions);
        params["success"] = serde_json::json!(success);
        self.client
            .send_notification::<crate::KnotCompilationComplete>(params)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::{Barrier, oneshot};

    #[tokio::test]
    async fn typing_invalidates_before_debounce_and_projects_are_independent() {
        let projects = Projects::default();
        let old = projects.begin("a".into()).await;
        let other = projects.begin("b".into()).await;
        let typed = projects.begin("a".into()).await;
        assert!(old.cancellation.is_cancelled());
        assert!(old.publication().await.is_none());
        assert!(typed.publication().await.is_some());
        assert!(other.publication().await.is_some());
        projects.cancel_all().await;
        assert!(typed.publication().await.is_none());
        assert!(other.publication().await.is_none());
    }

    #[tokio::test]
    async fn registration_waits_for_the_whole_publication() {
        let projects = Arc::new(Projects::default());
        let old = projects.begin("root".into()).await;
        let guard = old.publication().await.unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let (tx, mut rx) = oneshot::channel();
        let worker = tokio::spawn({
            let projects = Arc::clone(&projects);
            let barrier = Arc::clone(&barrier);
            async move {
                barrier.wait().await;
                tx.send(projects.begin("root".into()).await).ok();
            }
        });
        barrier.wait().await;
        // No scheduling delay is needed: holding the gate makes registration impossible.
        assert!(rx.try_recv().is_err());
        assert!(!old.cancellation.is_cancelled());
        drop(guard);
        let new = rx.await.unwrap();
        worker.await.unwrap();
        assert!(old.publication().await.is_none());
        assert!(new.publication().await.is_some());
    }

    #[tokio::test]
    async fn late_old_worker_cannot_overwrite_files_or_send_completion() {
        use futures::{FutureExt, StreamExt};
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        std::fs::write(
            root_path.join("knot.toml"),
            "[document]\nmain = 'main.knot'\n",
        )
        .unwrap();
        let path = root_path.join("main.knot");
        std::fs::write(&path, "old-source").unwrap();
        let uri = Url::from_file_path(&path).unwrap();
        let (service, mut socket) = service().await;
        let server = service.inner();
        let old = server.state.compilations.begin(root_path.clone()).await;
        let old_build =
            ProjectBuild::prepare(&root_path, &HashMap::new(), old.cancellation.clone()).unwrap();
        let old_output = old_build.compile(None).unwrap();
        let (release, released) = oneshot::channel();
        let worker = tokio::spawn({
            let server = server.clone_for_task();
            let uri = uri.clone();
            async move {
                released.await.unwrap();
                assert!(
                    !server
                        .publish_build(&old, &old_build, old_output, true)
                        .await
                );
                server
                    .finish_compile(&uri, &HashMap::new(), &old, true, true)
                    .await;
            }
        });
        std::fs::write(&path, "new-source").unwrap();
        let new = server.state.compilations.begin(root_path.clone()).await;
        let generation = new.id;
        server
            .run_compilation(uri, HashMap::new(), HashMap::new(), new, true)
            .await;
        let output = std::fs::read_to_string(root_path.join("main.typ")).unwrap();
        let metadata_path = knot_core::get_cache_dir(&root_path, &path).join("metadata.json");
        let metadata = std::fs::read(&metadata_path).unwrap();
        release.send(()).unwrap();
        worker.await.unwrap();
        assert!(output.contains("new-source"));
        assert!(!output.contains("old-source"));
        assert_eq!(
            std::fs::read_to_string(root_path.join("main.typ")).unwrap(),
            output
        );
        assert_eq!(std::fs::read(metadata_path).unwrap(), metadata);
        let message = socket.next().await.unwrap();
        assert_eq!(message.method(), "knot/compilationComplete");
        assert_eq!(message.params().unwrap()["generation"], generation);
        assert_eq!(message.params().unwrap()["success"], true);
        assert!(
            socket.next().now_or_never().is_none(),
            "obsolete worker sent a notification"
        );
    }

    #[tokio::test]
    async fn editor_sessions_use_and_follow_the_project_interpreters() {
        let root = tempfile::tempdir().unwrap();
        let toml = root.path().join("knot.toml");
        let main = root.path().join("main.knot");
        std::fs::write(&main, "").unwrap();
        let uri = Url::from_file_path(&main).unwrap();
        let (service, _socket) = service().await;
        let server = service.inner();
        for configured in ["./missing interpreter", "./other interpreter"] {
            std::fs::write(&toml, format!("[tools]\npython = '{configured}'\n")).unwrap();
            server.sync_with_cache(&uri).await;
            let mut executors = server.state.executors.write().await;
            let error = executors
                .get_mut(&uri)
                .unwrap()
                .get_executor("python")
                .err()
                .expect("a missing configured interpreter cannot start");
            // Never the system interpreter: the configured path is the one tried.
            let name = configured.trim_start_matches("./");
            assert!(format!("{error:#}").contains(name), "{error:#}");
        }
    }

    async fn service() -> (
        tower_lsp::LspService<KnotLanguageServer>,
        tower_lsp::ClientSocket,
    ) {
        let (mut service, socket) = tower_lsp::LspService::new(|client| KnotLanguageServer {
            client,
            state: crate::state::ServerState::new(),
            root_uri: Arc::new(tokio::sync::RwLock::new(None)),
        });
        let request = tower_lsp::jsonrpc::Request::build("initialize")
            .id(1)
            .params(serde_json::json!({"capabilities":{}}))
            .finish();
        tower::Service::call(&mut service, request).await.unwrap();
        (service, socket)
    }

    #[tokio::test]
    async fn preparation_and_publication_failures_report_failed_completion() {
        use futures::StreamExt;
        for missing_source in [true, false] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(
                root.path().join("knot.toml"),
                "[document]\nmain = 'main.knot'\n",
            )
            .unwrap();
            let path = root.path().join("main.knot");
            if !missing_source {
                std::fs::write(&path, "content").unwrap();
                // Force atomic replacement to fail after successful rendering.
                std::fs::create_dir(root.path().join("main.typ")).unwrap();
            }
            let uri = Url::from_file_path(path).unwrap();
            let (service, mut socket) = service().await;
            let server = service.inner();
            let request = server
                .state
                .compilations
                .begin(root.path().canonicalize().unwrap())
                .await;
            server
                .run_compilation(uri, HashMap::new(), HashMap::new(), request, true)
                .await;
            let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(message.method(), "knot/compilationComplete");
            assert_eq!(message.params().unwrap()["success"], false);
        }
    }

    #[tokio::test]
    async fn main_and_include_share_a_snapshot_but_another_project_does_not() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        for dir in [&root, &other] {
            std::fs::write(
                dir.path().join("knot.toml"),
                "[document]\nmain = 'main.knot'\nincludes = ['include.knot']\n",
            )
            .unwrap();
            std::fs::write(dir.path().join("main.knot"), "disk").unwrap();
            std::fs::write(dir.path().join("include.knot"), "disk").unwrap();
        }
        let (service, _) = service().await;
        let server = service.inner();
        for (path, text, version) in [
            (root.path().join("main.knot"), "main-buffer", 3),
            (root.path().join("include.knot"), "include-buffer", 7),
            (other.path().join("main.knot"), "other-buffer", 9),
        ] {
            server
                .update_document(&Url::from_file_path(path).unwrap(), text, version)
                .await;
        }
        let root = root.path().canonicalize().unwrap();
        let (buffers, versions) = server.open_buffers(&root).await;
        assert_eq!(buffers.len(), 2);
        assert_eq!(versions.len(), 2);
        let build = ProjectBuild::prepare(&root, &buffers, Default::default()).unwrap();
        let output = build.compile(None).unwrap();
        assert!(output.typ_content.contains("main-buffer"));
        assert!(output.typ_content.contains("include-buffer"));
        assert!(!output.typ_content.contains("other-buffer"));
    }
    #[tokio::test]
    async fn old_document_updates_and_versioned_diagnostics_are_rejected() {
        let (service, _socket) = service().await;
        let server = service.inner();
        let uri = Url::parse("file:///main.knot").unwrap();
        assert!(server.update_document(&uri, "new", 5).await);
        assert!(!server.update_document(&uri, "old", 4).await);
        let notify = |version, message: &str| {
            serde_json::json!({
                "method":"textDocument/publishDiagnostics", "params":{
                    "uri":crate::transform::to_virtual_uri(&uri), "version":version,
                    "diagnostics":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":message}]
                }
            })
        };
        server
            .handle_tinymist_notification(notify(5, "current"))
            .await;
        server
            .handle_tinymist_notification(notify(4, "obsolete"))
            .await;
        let docs = server.state.documents.read().await;
        assert_eq!(docs[&uri].text, "new");
        assert_eq!(docs[&uri].tinymist_diagnostics[0].message, "current");
    }
}
