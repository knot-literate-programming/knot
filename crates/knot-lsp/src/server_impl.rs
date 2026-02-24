//! KnotLanguageServer helper implementations
//!
//! This module contains the server's implementation details, separated from the
//! LSP protocol layer (`main.rs`). Three groups of concerns:
//!
//! - **Document management**: updating in-memory document state and publishing diagnostics.
//! - **Tinymist synchronization**: forwarding text-document notifications and
//!   routing notification messages from the Tinymist subprocess.
//! - **Cache/snapshot sync**: loading executor sessions from the on-disk cache.

use std::sync::Arc;

use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::get_cache_dir;
use tower_lsp::lsp_types::{MessageType, Range, Url};

use crate::diagnostics::get_diagnostics;
use crate::lsp_methods::{text_document as lsp, window as win};
use crate::position_mapper::PositionMapper;
use crate::transform;
use crate::transform::transform_to_typst;
use crate::{FormatChunkParams, KnotLanguageServer, StartPreviewParams, SyncForwardParams};

// ---------------------------------------------------------------------------
// Custom request handling
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    pub(crate) async fn handle_custom_format_chunk(
        &self,
        params: FormatChunkParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "LSP: Received request knot/formatChunk at line {}",
                    params.position.line
                ),
            )
            .await;
        match crate::handlers::formatting::handle_format_chunk(
            &self.state,
            &params.uri,
            params.position,
        )
        .await
        {
            Ok(Some(edit)) => {
                let _ = self.client.apply_edit(edit).await;
                Ok(serde_json::json!({"status": "success"}))
            }
            Ok(None) => Ok(serde_json::json!({"status": "no_changes"})),
            Err(_) => Err(tower_lsp::jsonrpc::Error::internal_error()),
        }
    }

    /// Start a preview task in our tinymist subprocess and return the static server port.
    ///
    /// On the first call, sends `tinymist.doStartPreview` to our subprocess with a
    /// fixed task ID ("knot-preview") and stores `(task_id, port)` for reuse by
    /// `do_sync_forward`. On subsequent calls the cached port is returned immediately.
    pub(crate) async fn handle_start_preview(
        &self,
        params: StartPreviewParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        match self.do_start_preview(&params).await {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("[startPreview] {e}");
                Ok(serde_json::json!({"status": "error", "message": e.to_string()}))
            }
        }
    }

    async fn do_start_preview(
        &self,
        params: &StartPreviewParams,
    ) -> anyhow::Result<serde_json::Value> {
        use anyhow::Context as _;

        // Return cached info if the preview is already running.
        if let Some(port) = self.state.preview_info.read().await.as_ref().map(|(_, p)| *p) {
            return Ok(serde_json::json!({"status": "ok", "staticServerPort": port}));
        }

        let knot_path = params
            .uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Invalid URI: {}", params.uri))?;

        let (config, project_root) = knot_core::config::Config::find_and_load(&knot_path)
            .context("Could not find knot.toml")?;

        let main_file = config.document.main.as_deref().unwrap_or("main.knot");
        let main_stem = std::path::Path::new(main_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");
        let main_typ_path = project_root.join(format!("{main_stem}.typ"));
        let main_typ_str = main_typ_path.to_string_lossy().to_string();

        const TASK_ID: &str = "knot-preview";

        // Pass --static-file-host 127.0.0.1:0 so older tinymist versions (which
        // have a separate static file server) don't default to fixed port 23627
        // (already in use by the tinymist VS Code extension). Passing 0 for both
        // hosts is also accepted by newer tinymist (they're treated as the same
        // server when equal). --dont-open-in-browser: the extension opens it.
        let response = {
            let mut tinymist_guard = self.state.tinymist.write().await;
            let proxy = tinymist_guard
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Tinymist not ready"))?;
            proxy
                .send_request_timeout(
                    "workspace/executeCommand",
                    serde_json::json!({
                        "command": "tinymist.doStartPreview",
                        "arguments": [[
                            "--task-id", TASK_ID,
                            "--data-plane-host", "127.0.0.1:0",
                            "--static-file-host", "127.0.0.1:0",
                            "--no-open",
                            &main_typ_str,
                        ]]
                    }),
                    30,
                )
                .await?
        };

        let result = response
            .get("result")
            .ok_or_else(|| anyhow::anyhow!("No result from tinymist.doStartPreview: {response:?}"))?;

        let static_server_port = result
            .get("staticServerPort")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing staticServerPort in: {result:?}"))?
            as u16;

        *self.state.preview_info.write().await = Some((TASK_ID.to_string(), static_server_port));

        log::info!("[startPreview] Preview started on port {static_server_port} (task={TASK_ID})");

        Ok(serde_json::json!({"status": "ok", "staticServerPort": static_server_port}))
    }

    /// Forward sync: map a `.knot` cursor position to the corresponding `.typ` line
    /// and scroll the tinymist preview to that position.
    pub(crate) async fn handle_sync_forward(
        &self,
        params: SyncForwardParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        match self.do_sync_forward(&params).await {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("[syncForward] {e}");
                Ok(serde_json::json!({"status": "error", "message": e.to_string()}))
            }
        }
    }

    async fn do_sync_forward(
        &self,
        params: &SyncForwardParams,
    ) -> anyhow::Result<serde_json::Value> {
        use anyhow::Context as _;

        let knot_path = params
            .uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Invalid URI: {}", params.uri))?;

        let (config, project_root) = knot_core::config::Config::find_and_load(&knot_path)
            .context("Could not find knot.toml")?;

        let main_file = config.document.main.as_deref().unwrap_or("main.knot");
        let main_stem = std::path::Path::new(main_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");
        let main_typ_path = project_root.join(format!("{main_stem}.typ"));

        let typ_content = std::fs::read_to_string(&main_typ_path)
            .with_context(|| format!("Could not read {}", main_typ_path.display()))?;

        let blocks = knot_core::sync::parse_knot_markers(&typ_content);

        let knot_rel = knot_path
            .strip_prefix(&project_root)
            .unwrap_or(&knot_path)
            .to_string_lossy()
            .to_string();

        // Both params.line (VSCode) and map_knot_line_to_typ input/output are 0-based.
        let knot_line_0based = params.line as usize;
        let Some(typ_line_0based) =
            knot_core::sync::map_knot_line_to_typ(&knot_rel, knot_line_0based, &blocks, &knot_path)
        else {
            // Cursor is in a Typst-only region — no mapping available.
            return Ok(serde_json::json!({"status": "unmapped"}));
        };
        let filepath = main_typ_path.to_string_lossy().to_string();

        log::info!(
            "[syncForward] {}:{} → {}:{}",
            knot_rel,
            knot_line_0based,
            filepath,
            typ_line_0based
        );

        // Scroll the preview if one is running in our tinymist subprocess.
        let preview_info = self.state.preview_info.read().await.clone();
        if let Some((task_id, _)) = preview_info {
            let scroll_result = {
                let mut tinymist_guard = self.state.tinymist.write().await;
                if let Some(proxy) = tinymist_guard.as_mut() {
                    proxy
                        .send_request(
                            "workspace/executeCommand",
                            serde_json::json!({
                                "command": "tinymist.scrollPreview",
                                "arguments": [task_id, {
                                    "event": "panelScrollTo",
                                    "filepath": filepath,
                                    "line": typ_line_0based,
                                    "character": 0,
                                }]
                            }),
                        )
                        .await
                } else {
                    Ok(serde_json::Value::Null)
                }
            };
            if let Err(e) = scroll_result {
                log::warn!("[syncForward] scroll failed: {e}");
            }
        }

        Ok(serde_json::json!({
            "status": "ok",
            "filepath": filepath,
            "typ_line": typ_line_0based,
        }))
    }

    /// Create a clonable `Arc` handle to `self` for use in `tokio::spawn` tasks.
    pub(crate) fn clone_for_task(&self) -> Arc<Self> {
        Arc::new(KnotLanguageServer {
            client: self.client.clone(),
            state: self.state.clone(),
            root_uri: self.root_uri.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Document management
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    /// Insert or update the in-memory document state for `uri`.
    pub(crate) async fn update_document(&self, uri: &Url, text: &str) {
        let typ_text = transform_to_typst(text);
        let mapper = PositionMapper::new(text, &typ_text);
        let knot_diagnostics = get_diagnostics(uri, text);

        let mut docs = self.state.documents.write().await;
        if let Some(doc) = docs.get_mut(uri) {
            doc.text = text.to_string();
            doc.mapper = mapper;
            doc.knot_diagnostics = knot_diagnostics;
            doc.version += 1;
        } else {
            docs.insert(
                uri.clone(),
                crate::state::DocumentState {
                    text: text.to_string(),
                    version: 1,
                    mapper,
                    opened_in_tinymist: false,
                    virtual_version: 0,
                    knot_diagnostics,
                    tinymist_diagnostics: Vec::new(),
                    formatting_error_notified: false,
                },
            );
        }
    }

    /// Merge Knot and Tinymist diagnostics and publish to the LSP client.
    pub(crate) async fn publish_combined_diagnostics(&self, uri: &Url) {
        let docs = self.state.documents.read().await;
        if let Some(doc) = docs.get(uri) {
            let mut combined = doc.knot_diagnostics.clone();
            combined.extend(doc.tinymist_diagnostics.clone());
            let _ = self
                .client
                .publish_diagnostics(uri.clone(), combined, None)
                .await;
        }
    }
}

// ---------------------------------------------------------------------------
// Tinymist synchronization
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    /// Translates a virtual URI back to the original `.knot` URI.
    ///
    /// Tinymist normalizes `knot-virtual://` URIs to `file://` on receipt, so
    /// `publishDiagnostics` arrives as `file://…foo.knot.typ`. We handle both.
    pub(crate) fn resolve_virtual_uri(&self, uri: &Url) -> Url {
        let path = uri.path().to_string();
        if uri.scheme() == "knot-virtual" || path.ends_with(".knot.typ") {
            let stripped = path.trim_end_matches(".typ").to_string();
            let mut original_uri = uri.clone();
            let _ = original_uri.set_scheme("file");
            original_uri.set_path(&stripped);
            return original_uri;
        }
        uri.clone()
    }

    /// Forward a text-document notification to Tinymist, transforming the content
    /// to Typst first. Marks the document as opened in Tinymist on first send, then
    /// warms up Tinymist's index with a synthetic document-symbol request.
    pub(crate) async fn forward_to_tinymist(&self, method: &str, uri: &Url) {
        let mut warm_up = false;

        // 1. Prepare data under a short-lived lock
        let sync_data = {
            let docs = self.state.documents.read().await;
            docs.get(uri)
                .map(|doc| (doc.text.clone(), doc.version, doc.opened_in_tinymist))
        };

        let (content, version, is_opened) = match sync_data {
            Some(data) => data,
            None => return,
        };

        let virtual_uri = transform::to_virtual_uri(uri);

        let params = if method == "didOpen" || !is_opened {
            serde_json::json!({ "textDocument": { "uri": virtual_uri, "languageId": "typst", "version": version, "text": transform_to_typst(&content) } })
        } else {
            serde_json::json!({ "textDocument": { "uri": virtual_uri, "version": version }, "contentChanges": [{ "text": transform_to_typst(&content) }] })
        };

        let actual_method = if !is_opened { lsp::DID_OPEN } else { method };

        // 2. Send to Tinymist (no state locks held)
        let send_result = {
            let mut tinymist_guard = self.state.tinymist.write().await;
            if let Some(proxy) = tinymist_guard.as_mut() {
                proxy.send_notification(actual_method, params).await
            } else {
                return;
            }
        };

        // 3. Update state if successful
        if send_result.is_ok() && !is_opened {
            let mut docs = self.state.documents.write().await;
            if let Some(doc) = docs.get_mut(uri) {
                doc.opened_in_tinymist = true;
                warm_up = true;
            }
        }

        if warm_up {
            let mut tinymist_guard = self.state.tinymist.write().await;
            if let Some(proxy) = tinymist_guard.as_mut() {
                let _ = proxy
                    .send_request(
                        lsp::DOCUMENT_SYMBOL,
                        serde_json::json!({ "textDocument": { "uri": virtual_uri } }),
                    )
                    .await;
            }
        }
    }

    /// Route an incoming notification from the Tinymist subprocess.
    pub(crate) async fn handle_tinymist_notification(&self, msg: serde_json::Value) {
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            return;
        };

        match method {
            lsp::PUBLISH_DIAGNOSTICS => self.handle_tinymist_diagnostics(&msg).await,
            win::SHOW_MESSAGE | win::LOG_MESSAGE => {
                self.handle_tinymist_message(method, &msg).await;
            }
            win::SHOW_DOCUMENT => self.handle_tinymist_show_document(&msg).await,
            _ => {}
        }
    }

    async fn handle_tinymist_diagnostics(&self, msg: &serde_json::Value) {
        if let Some(params) = msg.get("params")
            && let (Some(uri_str), Some(diagnostics_val)) = (
                params.get("uri").and_then(|u| u.as_str()),
                params.get("diagnostics"),
            )
            && let (Ok(virtual_uri), Ok(mut diagnostics)) = (
                Url::parse(uri_str),
                serde_json::from_value::<Vec<tower_lsp::lsp_types::Diagnostic>>(
                    diagnostics_val.clone(),
                ),
            )
        {
            let uri = self.resolve_virtual_uri(&virtual_uri);
            let mut docs = self.state.documents.write().await;
            if let Some(doc) = docs.get_mut(&uri) {
                for d in &mut diagnostics {
                    if let (Some(start), Some(end)) = (
                        doc.mapper.typ_to_knot_position(d.range.start),
                        doc.mapper.typ_to_knot_position(d.range.end),
                    ) {
                        d.range = Range { start, end };
                    }
                }
                doc.tinymist_diagnostics = diagnostics;
            }
            drop(docs);
            self.publish_combined_diagnostics(&uri).await;
        }
    }

    /// Handle a `window/showDocument` request sent by the Tinymist subprocess when the
    /// user clicks in the browser preview (backward sync: preview → .knot source).
    ///
    /// Flow:
    /// 1. Immediately acknowledge the request to tinymist.
    /// 2. Parse the .typ URI and line from the request params.
    /// 3. Map the .typ line → .knot file + line via the sync markers.
    /// 4. Ask VS Code to open the .knot file at the mapped position.
    async fn handle_tinymist_show_document(&self, msg: &serde_json::Value) {
        use tower_lsp::lsp_types::{Position, Range, ShowDocumentParams};

        // 1. Acknowledge tinymist immediately (it's a request, not a notification).
        let id = msg.get("id").cloned().unwrap_or(serde_json::Value::Null);
        {
            let mut tinymist_guard = self.state.tinymist.write().await;
            if let Some(proxy) = tinymist_guard.as_mut() {
                let _ = proxy
                    .send_raw_response(&id, serde_json::json!({"success": true}))
                    .await;
            }
        }

        // 2. Extract the URI and line number from the request.
        let Some(params) = msg.get("params") else { return };
        let Some(uri_str) = params.get("uri").and_then(|u| u.as_str()) else { return };
        let Ok(uri) = Url::parse(uri_str) else { return };

        // The selection in the .typ file (start line, 0-based).
        let typ_line = params
            .get("selection")
            .and_then(|s| s.get("start"))
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .unwrap_or(0) as usize;

        // 3. Resolve the file path and load sync markers.
        // The URI may be the disk .typ file (from preview) or a virtual .knot.typ URI.
        let typ_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        let project_root = match knot_core::config::Config::find_project_root(&typ_path) {
            Ok(r) => r,
            Err(_) => return,
        };

        let typ_content = match std::fs::read_to_string(&typ_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let blocks = knot_core::sync::parse_knot_markers(&typ_content);

        let Some((knot_path, knot_line_0based)) =
            knot_core::sync::map_typ_line_to_knot(typ_line, &blocks, &project_root)
        else {
            log::debug!("[showDocument] No .knot mapping for {uri_str}:{typ_line}");
            return;
        };

        log::info!(
            "[showDocument] {}:{} → {}:{}",
            uri_str,
            typ_line,
            knot_path.display(),
            knot_line_0based
        );

        // 4. Ask VS Code to open the .knot file and jump to the mapped line.
        let Ok(knot_uri) = Url::from_file_path(&knot_path) else { return };
        let pos = Position {
            line: knot_line_0based as u32,
            character: 0,
        };
        let _ = self
            .client
            .show_document(ShowDocumentParams {
                uri: knot_uri,
                external: Some(false),
                take_focus: Some(true),
                selection: Some(Range { start: pos, end: pos }),
            })
            .await;
    }

    async fn handle_tinymist_message(&self, method: &str, msg: &serde_json::Value) {
        if let Some(params) = msg.get("params")
            && let Some(type_num) = params.get("type").and_then(|t| t.as_u64())
            && let Some(message) = params.get("message").and_then(|m| m.as_str())
        {
            let msg_type = match type_num {
                1 => MessageType::ERROR,
                2 => MessageType::WARNING,
                3 => MessageType::INFO,
                _ => MessageType::LOG,
            };
            let prefixed = format!("[Tinymist] {message}");
            if method == win::SHOW_MESSAGE {
                self.client.show_message(msg_type, prefixed).await;
            } else {
                self.client.log_message(msg_type, prefixed).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cache / snapshot synchronisation
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    /// Poll `metadata.json` until its mtime changes (compilation wrote new data),
    /// then re-read diagnostics from the updated cache and publish them.
    ///
    /// This bridges the gap between `knot build` / `knot watch` updating the
    /// on-disk cache and the LSP client seeing the new errors.  Without this,
    /// errors are only shown after the user next types something in the editor.
    ///
    /// Times out after 30 s so the background task does not linger if the
    /// compiler is not running.
    pub(crate) async fn refresh_diagnostics_on_cache_update(&self, uri: &Url) {
        let Some(metadata_path) = self.cache_metadata_path(uri) else {
            return;
        };

        let initial_mtime = std::fs::metadata(&metadata_path)
            .and_then(|m| m.modified())
            .ok();

        // Poll every 500 ms for up to 30 seconds (60 attempts).
        for _ in 0..60u8 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            let current_mtime = std::fs::metadata(&metadata_path)
                .and_then(|m| m.modified())
                .ok();

            if current_mtime != initial_mtime {
                let text = {
                    let docs = self.state.documents.read().await;
                    docs.get(uri).map(|d| d.text.clone())
                };
                if let Some(text) = text {
                    self.update_document(uri, &text).await;
                    self.publish_combined_diagnostics(uri).await;
                }
                return;
            }
        }
    }

    /// Returns the path to `metadata.json` for the cache associated with `uri`.
    fn cache_metadata_path(&self, uri: &Url) -> Option<std::path::PathBuf> {
        let path = uri.to_file_path().ok()?;
        let project_root = Config::find_project_root(&path).ok()?;
        let file_stem = path.file_stem().and_then(|s| s.to_str())?;
        let cache_dir = get_cache_dir(&project_root, file_stem);
        Some(cache_dir.join("metadata.json"))
    }
}

impl KnotLanguageServer {
    /// Load the most recent executor session snapshots from the on-disk cache.
    pub(crate) async fn sync_with_cache(&self, uri: &Url) {
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        let project_root = match Config::find_project_root(&path) {
            Ok(root) => root,
            Err(_) => return,
        };
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);
        if let Ok(cache) = Cache::new(cache_dir) {
            self.try_load_snapshot(uri, &cache, "r", "RData").await;
            self.try_load_snapshot(uri, &cache, "python", "pkl").await;
        }
    }

    /// Reload the most recent executor session snapshot for one language if it
    /// differs from the one already loaded (avoids redundant I/O on every save).
    async fn try_load_snapshot(&self, uri: &Url, cache: &Cache, language: &str, extension: &str) {
        let last_chunk = match cache
            .metadata
            .chunks
            .iter()
            .filter(|c| c.language == language && cache.has_snapshot(&c.hash, extension))
            .max_by_key(|c| c.index)
        {
            Some(c) => c,
            None => return,
        };

        let reload_key = format!("{}::{}", uri, language);

        if self
            .state
            .loaded_snapshot_hash
            .read()
            .await
            .get(&reload_key)
            == Some(&last_chunk.hash)
        {
            return; // Already up to date
        }

        let snapshot_path = cache.get_snapshot_path(&last_chunk.hash, extension);
        let chunk_index = last_chunk.index;

        if let Some(manager) = self.state.executors.write().await.get_mut(uri)
            && let Ok(executor) = manager.get_executor(language)
            && executor.load_session(&snapshot_path).is_ok()
        {
            self.state
                .loaded_snapshot_hash
                .write()
                .await
                .insert(reload_key, last_chunk.hash.clone());
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "Synced {} session (chunk {})",
                        language.to_uppercase(),
                        chunk_index
                    ),
                )
                .await;
        }
    }
}
