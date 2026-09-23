//! KnotLanguageServer helper implementations
//!
//! This module contains the server's implementation details, separated from the
//! LSP protocol layer (`main.rs`). Three groups of concerns:
//!
//! - **Document management**: updating in-memory document state and publishing diagnostics.
//! - **Tinymist synchronization**: forwarding text-document notifications and
//!   routing notification messages from the Tinymist subprocess.
//! - **Cache/snapshot sync**: loading executor sessions from the on-disk cache.

use std::path::Path;
use std::sync::Arc;

use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::executors::ExecutorManager;
use knot_core::get_cache_dir;
use tower_lsp::lsp_types::{MessageType, Range, Url};

use crate::diagnostics::get_diagnostics;
use crate::lsp_methods::{text_document as lsp, window as win};
use crate::position_mapper::PositionMapper;
use crate::state::TinymistOverlay;
use crate::transform;
use crate::transform::transform_to_typst;
use crate::{
    CompileParams, FormatChunkParams, KnotLanguageServer, StartPreviewParams, SyncForwardParams,
};

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
                let response = self.client.apply_edit(edit).await?;
                if !response.applied {
                    return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                        "Editor did not apply formatting: {}",
                        response
                            .failure_reason
                            .as_deref()
                            .unwrap_or("edit rejected")
                    )));
                }
                Ok(serde_json::json!({"status": "success"}))
            }
            Ok(None) => Ok(serde_json::json!({"status": "no_changes"})),
            Err(error) => Err(error),
        }
    }

    /// Trigger a full streaming compile explicitly (e.g. from the "Run" button).
    ///
    /// Uses the same project coordinator as save and typing, superseding older work.
    pub(crate) async fn handle_compile(
        &self,
        params: CompileParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        self.queue_compile(&params.uri, true, false).await;

        Ok(serde_json::json!({"status": "ok"}))
    }

    /// Start a preview task in our tinymist subprocess and return the static server port.
    ///
    /// On the first call, sends `tinymist.doStartPreview` to our subprocess with a
    /// project-specific task ID and stores `(task_id, port)` for reuse by
    /// `do_sync_forward`. On subsequent calls the cached port is returned immediately.
    pub(crate) async fn handle_start_preview(
        &self,
        params: StartPreviewParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        log::info!("[startPreview] Request received for URI: {}", params.uri);
        match self.do_start_preview(&params).await {
            Ok(result) => {
                log::info!("[startPreview] Success: {:?}", result);
                Ok(result)
            }
            Err(e) => {
                log::warn!("[startPreview] Error: {e}");
                Ok(serde_json::json!({"status": "error", "message": e.to_string()}))
            }
        }
    }

    async fn do_start_preview(
        &self,
        params: &StartPreviewParams,
    ) -> anyhow::Result<serde_json::Value> {
        use anyhow::Context as _;
        let knot_path = params
            .uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Invalid URI"))?;
        let (config, root) = Config::find_and_load(&knot_path)?;
        let root = root.canonicalize()?;
        let main_typ_path = knot_core::ProjectPaths::resolve(&config, &root)?.main_typ_path;
        let project = self.state.compilations.project(root.clone()).await;
        // Only one preview startup per project. Never hold the publication gate
        // across the potentially slow doStartPreview request.
        let _startup = project.preview_start.lock().await;
        let existing = self.state.preview_info.read().await.get(&root).cloned();
        let gate = project.gate.lock().await;
        if let Some((task_id, port)) = &existing
            && main_typ_path.exists()
        {
            return Ok(
                serde_json::json!({"status":"ok", "staticServerPort":port, "taskId":task_id}),
            );
        }
        let typ_content = if main_typ_path.exists() {
            std::fs::read_to_string(&main_typ_path)?
        } else {
            let (buffers, _) = self.open_buffers(&root).await;
            let path = root.clone();
            let (build, output) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                let build =
                    knot_core::project::ProjectBuild::prepare(&path, &buffers, Default::default())?;
                let output = build.phase0(knot_core::Phase0Mode::Pending)?;
                Ok((build, output))
            })
            .await??;
            build.publish(&output, false)?;
            output.typ_content
        };
        let uri = Url::from_file_path(&main_typ_path)
            .map_err(|_| anyhow::anyhow!("Invalid Typst path"))?;
        let proxy = self
            .state
            .tinymist
            .read()
            .await
            .as_ref()
            .cloned()
            .context("Tinymist not ready")?;
        if self
            .state
            .tinymist_overlay
            .read()
            .await
            .contains_key(&main_typ_path)
        {
            self.send_overlay(&typ_content, &main_typ_path).await?;
        } else {
            proxy
                .send_notification(
                    lsp::DID_OPEN,
                    serde_json::json!({"textDocument":{
                        "uri":uri,"languageId":"typst","version":1,"text":typ_content
                    }}),
                )
                .await?;
            self.state.tinymist_overlay.write().await.insert(
                main_typ_path.clone(),
                TinymistOverlay::Active { next_version: 2 },
            );
        }
        drop(gate);
        if let Some((task_id, port)) = existing {
            self.queue_compile(&params.uri, true, false).await;
            return Ok(
                serde_json::json!({"status":"ok", "staticServerPort":port, "taskId":task_id}),
            );
        }
        let task_id = format!("knot-preview-{}", uuid::Uuid::new_v4());
        let response = proxy
            .send_request_timeout(
                "workspace/executeCommand",
                serde_json::json!({
                    "command":"tinymist.doStartPreview", "arguments":[[
                        "--task-id",task_id,"--data-plane-host","127.0.0.1:0",
                        "--control-plane-host","127.0.0.1:0","--static-file-host","127.0.0.1:0",
                        "--no-open",main_typ_path.to_string_lossy()
                    ]]
                }),
                30,
            )
            .await?;
        let port = response
            .pointer("/result/staticServerPort")
            .and_then(|p| p.as_u64())
            .context("Missing preview port")? as u16;
        self.state
            .preview_info
            .write()
            .await
            .insert(root, (task_id.clone(), port));
        // No replay of the startup content here: newer results may already have
        // been published while the preview request was in flight.
        self.queue_compile(&params.uri, true, false).await;
        Ok(serde_json::json!({"status":"ok", "staticServerPort":port, "taskId":task_id}))
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

        let main_typ_path = knot_core::ProjectPaths::resolve(&config, &project_root)?.main_typ_path;
        let filepath = main_typ_path.to_string_lossy().to_string();

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

        log::info!(
            "[syncForward] {}:{} → {}:{}",
            knot_rel,
            knot_line_0based,
            filepath,
            typ_line_0based
        );

        // Find the best scrollable line: the mapped line if it contains Typst text,
        // otherwise scan backwards for the nearest preceding text line.
        //
        // `jump_from_cursor` in tinymist uses `leaf_at(cursor, Side::Before)`:
        // - At column 0, it finds the node BEFORE the cursor (previous line) → fails.
        // - On `#func(...)` or `// comment` lines, there is no Text node → fails.
        // We fix both by (a) using col+1 to be inside a Text node, and (b) scanning
        // back past code chunks / markers to the nearest real text line.
        let (scroll_line, character) = {
            let lines: Vec<&str> = typ_content.lines().collect();
            let scan_from = typ_line_0based.min(lines.len().saturating_sub(1));

            let mut found = None;
            for i in (0..=scan_from).rev() {
                let Some(line) = lines.get(i) else { break };
                let trimmed = line.trim_start();
                // Stop at file-block boundaries (don't cross into another file).
                if trimmed.starts_with("// BEGIN-FILE") || trimmed.starts_with("// END-FILE") {
                    break;
                }
                // Skip comments, Typst function calls (#...), and empty lines —
                // none of these contain a Text AST node.
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                    continue;
                }
                // Headings in Typst look like `== Title`. The `=` chars
                // form a HeadingMarker AST node, not a Text node.
                // jump_from_cursor only accepts Text/MathText nodes, so we
                // must land the cursor inside the title text, not on the `=`.
                let col = if trimmed.starts_with('=') {
                    line.find(|c: char| c != '=' && !c.is_whitespace())
                        .unwrap_or(3)
                } else {
                    line.find(|c: char| !c.is_whitespace()).unwrap_or(0)
                };
                found = Some((i, (col + 1) as u32));
                break;
            }

            found.unwrap_or((scan_from, 1))
        };

        // Scroll the preview if one is running in our tinymist subprocess.
        let root = project_root.canonicalize()?;
        let preview_info = self.state.preview_info.read().await.get(&root).cloned();
        if let Some((task_id, _)) = preview_info {
            let proxy = self.state.tinymist.read().await.as_ref().cloned();

            if let Some(proxy) = proxy {
                log::info!(
                    "[syncForward] Sending scroll: knot→typ:{} → scroll_line={} character={}",
                    typ_line_0based,
                    scroll_line,
                    character
                );

                let _ = proxy
                    .send_request(
                        "workspace/executeCommand",
                        serde_json::json!({
                            "command": "tinymist.scrollPreview",
                            "arguments": [task_id, {
                                "event": "panelScrollTo",
                                "filepath": filepath,
                                "line": scroll_line as u32,
                                "character": character,
                            }]
                        }),
                    )
                    .await;
            }
        } else {
            log::info!("[syncForward] No preview running, skipping scroll");
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
// Preview pipeline
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    /// Send the overlay part of a publication while the project's gate is held.
    pub(crate) async fn send_overlay(
        &self,
        content: &str,
        main_typ_path: &Path,
    ) -> anyhow::Result<()> {
        let version = {
            let mut overlays = self.state.tinymist_overlay.write().await;
            overlays
                .get_mut(main_typ_path)
                .map(|TinymistOverlay::Active { next_version }| {
                    let version = *next_version;
                    *next_version += 1;
                    version
                })
        };
        if let (Some(version), Ok(uri)) = (version, Url::from_file_path(main_typ_path)) {
            let proxy = self.state.tinymist.read().await.as_ref().cloned();
            let proxy = proxy.ok_or_else(|| anyhow::anyhow!("Tinymist overlay has no proxy"))?;
            proxy.send_notification(lsp::DID_CHANGE, serde_json::json!({
                "textDocument": {"uri":uri,"version":version}, "contentChanges":[{"text":content}]
            })).await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Document management
// ---------------------------------------------------------------------------

impl KnotLanguageServer {
    /// Insert or update the in-memory document state for `uri`.
    pub(crate) async fn update_document(&self, uri: &Url, text: &str, version: i32) -> bool {
        let typ_text = transform_to_typst(text);
        let mapper = PositionMapper::new(text, &typ_text);
        let knot_diagnostics = get_diagnostics(uri, text, false);

        let mut docs = self.state.documents.write().await;
        if let Some(doc) = docs.get_mut(uri) {
            if version <= doc.version {
                return false;
            }
            doc.tinymist_diagnostics.clear();
            doc.text = text.to_string();
            doc.mapper = mapper;
            doc.knot_diagnostics = knot_diagnostics;
            doc.version = version;
        } else {
            docs.insert(
                uri.clone(),
                crate::state::DocumentState {
                    text: text.to_string(),
                    version,
                    mapper,
                    opened_in_tinymist: false,
                    knot_diagnostics,
                    tinymist_diagnostics: Vec::new(),
                },
            );
        }
        true
    }

    pub(crate) async fn refresh_runtime_diagnostics(
        &self,
        uri: &Url,
        versions: &std::collections::HashMap<String, i32>,
    ) {
        {
            let mut docs = self.state.documents.write().await;
            if let Some(doc) = docs.get_mut(uri) {
                if versions.get(uri.as_str()) != Some(&doc.version) {
                    return;
                }
                doc.knot_diagnostics = get_diagnostics(uri, &doc.text, true);
            }
        }
        self.publish_combined_diagnostics(uri).await;
    }

    /// Merge Knot and Tinymist diagnostics and publish to the LSP client.
    pub(crate) async fn publish_combined_diagnostics(&self, uri: &Url) {
        let docs = self.state.documents.read().await;
        if let Some(doc) = docs.get(uri) {
            let mut combined = doc.knot_diagnostics.clone();
            combined.extend(doc.tinymist_diagnostics.clone());
            let _ = self
                .client
                .publish_diagnostics(uri.clone(), combined, Some(doc.version))
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
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
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
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
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
                // Tinymist 0.15.2 omits versions even when versionSupport is set.
                // Preserve its diagnostics; reject provably stale versioned ones.
                // Unversioned upstream diagnostics cannot be correlated reliably.
                if let Some(version) = params.get("version").and_then(|v| v.as_i64())
                    && version != i64::from(doc.version)
                {
                    return;
                }
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
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
                let _ = proxy
                    .send_raw_response(&id, serde_json::json!({"success": true}))
                    .await;
            }
        }

        // 2. Extract the URI and line number from the request.
        let Some(params) = msg.get("params") else {
            return;
        };
        let Some(uri_str) = params.get("uri").and_then(|u| u.as_str()) else {
            return;
        };
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
        let Ok(knot_uri) = Url::from_file_path(&knot_path) else {
            return;
        };
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
                selection: Some(Range {
                    start: pos,
                    end: pos,
                }),
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
    /// Load the most recent executor session snapshots from the on-disk cache.
    ///
    /// Also ensures an [`ExecutorManager`] exists for this document.  The
    /// manager is used by hover and completion handlers to query running R /
    /// Python processes.  The executors themselves are started lazily on the
    /// first call to `get_executor("r")` / `get_executor("python")`.
    pub(crate) async fn sync_with_cache(&self, uri: &Url) {
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        let project_root = match Config::find_project_root(&path) {
            Ok(root) => root,
            Err(_) => return,
        };
        let cache_dir = get_cache_dir(&project_root, &path);

        // Lazy-init: create an ExecutorManager for this URI if one doesn't
        // exist yet (hover/completion in R/Python chunks require it).
        {
            let mut executors = self.state.executors.write().await;
            executors.entry(uri.clone()).or_insert_with(|| {
                ExecutorManager::new(cache_dir.clone()).with_working_directory(project_root.clone())
            });
        }

        if let Ok(cache) = Cache::new(cache_dir) {
            self.try_load_snapshot(uri, &cache, "r").await;
            self.try_load_snapshot(uri, &cache, "python").await;
        }
    }

    /// Reload the most recent executor session snapshot for one language if it
    /// differs from the one already loaded (avoids redundant I/O on every save).
    async fn try_load_snapshot(&self, uri: &Url, cache: &Cache, language: &str) {
        let reload_key = format!("{}::{}", uri, language);
        let allowed = {
            let documents = self.state.documents.read().await;
            documents.get(uri).is_none_or(|state| {
                let document = knot_core::Document::parse(state.text.clone());
                document.errors.is_empty()
                    && document.snapshots.get(language).copied().unwrap_or(true)
            })
        };
        let last_chunk = match cache
            .metadata
            .chunks
            .iter()
            .filter(|c| {
                allowed
                    && !cache
                        .metadata
                        .disabled_snapshot_languages
                        .contains(language)
                    && c.language == language
                    && c.error.is_none()
                    && cache.snapshot_is_valid(&c.hash)
            })
            .max_by_key(|c| c.index)
        {
            Some(c) => c,
            None => {
                if let Some(manager) = self.state.executors.write().await.get_mut(uri) {
                    drop(manager.take(language));
                }
                self.state
                    .loaded_snapshot_hash
                    .write()
                    .await
                    .remove(&reload_key);
                return;
            }
        };

        let Some(snapshot) = cache.metadata.snapshots.get(&last_chunk.hash) else {
            return;
        };
        let mut files: Vec<_> = snapshot.files.iter().collect();
        files.sort_unstable();
        // The same execution hash can acquire a new state after cache:false or
        // repair. Compare the validated contents, not just the source hash.
        let snapshot_identity = format!("{}:{files:?}", last_chunk.hash);

        if self
            .state
            .loaded_snapshot_hash
            .read()
            .await
            .get(&reload_key)
            == Some(&snapshot_identity)
        {
            return; // Already up to date
        }

        let chunk_index = last_chunk.index;

        let mut managers = self.state.executors.write().await;
        let Some(manager) = managers.get_mut(uri) else {
            return;
        };
        // Loading into a reused session would retain variables deleted since
        // the previous snapshot.
        drop(manager.take(language));
        let restored = manager
            .get_executor(language)
            .is_ok_and(|executor| cache.restore_snapshot(&last_chunk.hash, executor).is_ok());
        drop(managers);
        if restored {
            self.state
                .loaded_snapshot_hash
                .write()
                .await
                .insert(reload_key, snapshot_identity);
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
