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
use std::sync::atomic::Ordering;

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
                let _ = self.client.apply_edit(edit).await;
                Ok(serde_json::json!({"status": "success"}))
            }
            Ok(None) => Ok(serde_json::json!({"status": "no_changes"})),
            Err(_) => Err(tower_lsp::jsonrpc::Error::internal_error()),
        }
    }

    /// Trigger a full streaming compile explicitly (e.g. from the "Run" button).
    ///
    /// Behaves like `did_save` minus the `forward_to_tinymist` call: increments
    /// the compile generation (cancelling any stale in-flight compile) and spawns
    /// `do_compile`.  Also aborts any pending Phase-0 debounce so it doesn't
    /// race with the full compile.
    pub(crate) async fn handle_compile(
        &self,
        params: CompileParams,
    ) -> tower_lsp::jsonrpc::Result<serde_json::Value> {
        // Cancel any pending Phase-0 debounce.
        {
            let mut handles = self.state.debounce_handles.lock().await;
            if let Some(h) = handles.remove(&params.uri) {
                h.abort();
            }
        }

        let generation = self.state.compile_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone_for_task();
        let uri = params.uri.clone();
        tokio::spawn(async move {
            this.do_compile(&uri, generation).await;
        });

        Ok(serde_json::json!({"status": "ok"}))
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

        // Return cached info if the preview is already running.
        if let Some(port) = self
            .state
            .preview_info
            .read()
            .await
            .as_ref()
            .map(|(_, p)| *p)
        {
            return Ok(serde_json::json!({"status": "ok", "staticServerPort": port}));
        }

        let knot_path = params
            .uri
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Invalid URI: {}", params.uri))?;

        // ── 1. Resolve main.typ path from project config ───────────────────────
        let (config, project_root) = knot_core::config::Config::find_and_load(&knot_path)
            .context("Could not find knot.toml")?;
        let main_file_name = config
            .document
            .main
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No 'main' file in knot.toml"))?
            .to_string();
        let main_stem = Path::new(&main_file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid main filename: {main_file_name}"))?
            .to_string();
        let main_typ_path = project_root.join(format!("{main_stem}.typ"));
        let main_typ_uri = Url::from_file_path(&main_typ_path)
            .map_err(|_| anyhow::anyhow!("Cannot build URI for {}", main_typ_path.display()))?;
        let main_typ_str = main_typ_path.to_string_lossy().to_string();

        // ── 2. Get initial content for the overlay ─────────────────────────────
        // Re-use the existing main.typ (from a previous compilation) rather than
        // overwriting it with Phase 0 placeholders — this avoids rolling back a
        // streaming result that may already be on disk.  Only run Phase 0 when
        // main.typ does not exist yet (first open, fresh clone, after clean).
        let typ_content = if main_typ_path.exists() {
            log::info!("[startPreview] Using existing main.typ");
            std::fs::read_to_string(&main_typ_path).context("Failed to read existing main.typ")?
        } else {
            log::info!("[startPreview] main.typ not found — running Phase 0");
            let output = tokio::task::spawn_blocking({
                let path = knot_path.clone();
                move || knot_core::compile_project_phase0(&path, knot_core::Phase0Mode::Pending)
            })
            .await
            .map_err(|_| anyhow::anyhow!("Phase 0 panicked"))?
            .context("compile_project_phase0 failed")?;
            output.typ_content
        };

        const TASK_ID: &str = "knot-preview";

        // ── 3. textDocument/didOpen (v=1) + doStartPreview ────────────────────
        let response = {
            let tinymist_guard = self.state.tinymist.read().await;
            let proxy = tinymist_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Tinymist not ready"))?;

            // Open the overlay (version = 1).
            let _ = proxy
                .send_notification(
                    lsp::DID_OPEN,
                    serde_json::json!({
                        "textDocument": {
                            "uri": main_typ_uri,
                            "languageId": "typst",
                            "version": 1,
                            "text": &typ_content,
                        }
                    }),
                )
                .await;

            // Start the preview task. --no-open: the extension opens the browser.
            proxy
                .send_request_timeout(
                    "workspace/executeCommand",
                    serde_json::json!({
                        "command": "tinymist.doStartPreview",
                        "arguments": [[
                            "--task-id", TASK_ID,
                            "--data-plane-host", "127.0.0.1:0",
                            "--control-plane-host", "127.0.0.1:0",
                            "--static-file-host", "127.0.0.1:0",
                            "--no-open",
                            &main_typ_str,
                        ]]
                    }),
                    30,
                )
                .await?
        };

        // ── 4. Store preview_info ──────────────────────────────────────────────
        let result = response.get("result").ok_or_else(|| {
            anyhow::anyhow!("No result from tinymist.doStartPreview: {response:?}")
        })?;
        let static_server_port = result
            .get("staticServerPort")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing staticServerPort in: {result:?}"))?
            as u16;

        *self.state.preview_info.write().await = Some((TASK_ID.to_string(), static_server_port));

        // ── 5. Activate overlay + send didChange v=2 ──────────────────────────
        *self.state.tinymist_overlay.write().await = TinymistOverlay::Active { next_version: 3 };
        {
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
                let _ = proxy
                    .send_notification(
                        lsp::DID_CHANGE,
                        serde_json::json!({
                            "textDocument": { "uri": main_typ_uri, "version": 2 },
                            "contentChanges": [{ "text": &typ_content }],
                        }),
                    )
                    .await;
            }
        }

        // ── 6. Kick off a streaming compile ───────────────────────────────────
        // Ensures the browser transitions from the initial state (Phase 0
        // placeholders or previous result) to fully-executed output without
        // requiring an explicit save.  Uses the current generation so it
        // coexists gracefully with any ongoing did_save → do_compile.
        let generation = self.state.compile_generation.load(Ordering::SeqCst);
        let this = self.clone_for_task();
        let uri = params.uri.clone();
        tokio::spawn(async move {
            this.do_compile(&uri, generation).await;
        });

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
        let preview_info = self.state.preview_info.read().await.clone();
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
    /// Apply a preview update: write `content` to disk, then (if the Tinymist
    /// overlay is active) send a `textDocument/didChange` to the subprocess so
    /// the browser refreshes instantly — without waiting for macOS FSEvents.
    ///
    /// Does nothing if `generation` no longer matches `compile_generation`
    /// (a newer save arrived while this compilation was in progress).
    pub(crate) async fn apply_update(&self, content: &str, main_typ_path: &Path, generation: u64) {
        // Guard: abort if a newer save has superseded this compilation.
        if self.state.compile_generation.load(Ordering::SeqCst) != generation {
            return;
        }

        // 1. Write to disk (needed for sync forward, knot watch, typst, etc.)
        let _ = std::fs::write(main_typ_path, content);

        // 2. Get the next version from the overlay (write lock) + increment.
        let version_opt = {
            let mut overlay = self.state.tinymist_overlay.write().await;
            if let TinymistOverlay::Active { next_version } = &mut *overlay {
                let v = *next_version;
                *next_version += 1;
                Some(v)
            } else {
                None // Overlay not yet active (preview not started).
            }
        };

        // 3. Send textDocument/didChange to Tinymist (overlay active only).
        if let (Some(v), Ok(uri)) = (version_opt, Url::from_file_path(main_typ_path)) {
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
                let _ = proxy
                    .send_notification(
                        lsp::DID_CHANGE,
                        serde_json::json!({
                            "textDocument": { "uri": uri, "version": v },
                            "contentChanges": [{ "text": content }],
                        }),
                    )
                    .await;
            }
        }
    }

    /// Full compile pipeline triggered on every `did_save`:
    ///
    /// 1. **Phase 0** (~instant): plan only, cache hits rendered, MustExecute
    ///    chunks as placeholders → send to preview immediately.
    /// 2. **Streaming**: execute chunks one by one; after each, push a partial
    ///    assembled `.typ` to the preview.
    /// 3. **Final**: once all chunks are done, send the complete `.typ` and
    ///    update diagnostics.
    pub(crate) async fn do_compile(&self, uri: &Url, generation: u64) {
        let knot_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        // ── Phase 0 ──────────────────────────────────────────────────────────
        let phase0 = tokio::task::spawn_blocking({
            let path = knot_path.clone();
            move || knot_core::compile_project_phase0(&path, knot_core::Phase0Mode::Pending)
        })
        .await;

        let output = match phase0 {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                log::warn!("[do_compile] Phase 0 failed: {e}");
                return;
            }
            Err(_) => {
                log::warn!("[do_compile] Phase 0 panicked");
                return;
            }
        };

        let main_typ_path = output.main_typ_path.clone();
        self.apply_update(&output.typ_content, &main_typ_path, generation)
            .await;

        // Early exit if a newer save arrived.
        if self.state.compile_generation.load(Ordering::SeqCst) != generation {
            return;
        }

        // Notify the editor that a full compile is starting (enables status bar,
        // keeps Run button active).
        self.client
            .send_notification::<crate::KnotCompilationStarted>(
                serde_json::json!({"uri": uri.to_string()}),
            )
            .await;

        // ── Full compile with per-chunk streaming ─────────────────────────────
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let execute_handle = tokio::task::spawn_blocking({
            let path = knot_path.clone();
            move || {
                knot_core::compile_project_full(
                    &path,
                    Some(Box::new(move |typ| {
                        let _ = progress_tx.send(typ);
                    })),
                )
            }
        });

        // Receive and apply each partial assembled .typ.
        while let Some(partial_typ) = progress_rx.recv().await {
            self.apply_update(&partial_typ, &main_typ_path, generation)
                .await;
        }

        // ── Final result ──────────────────────────────────────────────────────
        let final_output = match execute_handle.await {
            Ok(Ok(o)) => o,
            result => {
                if let Ok(Err(e)) = &result {
                    log::warn!("[do_compile] Full compile failed: {e}");
                } else {
                    log::warn!("[do_compile] Full compile panicked");
                }
                self.client
                    .send_notification::<crate::KnotCompilationComplete>(
                        serde_json::json!({"uri": uri.to_string(), "success": false}),
                    )
                    .await;
                return;
            }
        };

        self.apply_update(&final_output.typ_content, &main_typ_path, generation)
            .await;

        // Diagnostics only when this is still the current generation.
        if self.state.compile_generation.load(Ordering::SeqCst) == generation {
            self.sync_with_cache(uri).await;
            self.publish_combined_diagnostics(uri).await;
        }

        self.client
            .send_notification::<crate::KnotCompilationComplete>(
                serde_json::json!({"uri": uri.to_string(), "success": true}),
            )
            .await;
    }

    /// Phase-0-only compile triggered by `did_change` debounce.
    ///
    /// Runs the planning pass only — **no chunk execution**.  Cache hits are
    /// rendered with their real output; modified chunks appear as placeholders.
    /// This is safe to call while the user is actively typing (including inside
    /// a code chunk with syntactically incomplete code) because nothing is
    /// executed and the cache is never written.
    pub(crate) async fn do_phase0_only(&self, uri: &Url) {
        let knot_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Get the current in-memory text (may differ from disk if unsaved).
        let text = {
            let docs = self.state.documents.read().await;
            docs.get(uri).map(|d| d.text.clone())
        };

        let result = tokio::task::spawn_blocking({
            let path = knot_path.clone();
            move || match text {
                // Use the in-memory buffer so unsaved edits are visible
                // immediately (without waiting for an explicit save).
                Some(t) => knot_core::compile_project_phase0_unsaved(
                    &path,
                    &path,
                    &t,
                    knot_core::Phase0Mode::Modified,
                ),
                // Fallback: document not in state yet, read from disk.
                None => knot_core::compile_project_phase0(&path, knot_core::Phase0Mode::Modified),
            }
        })
        .await;

        let Ok(Ok(output)) = result else { return };

        // Write to disk + send didChange to Tinymist overlay if active.
        // No generation guard: Phase 0 is idempotent and never corrupts state.
        let _ = std::fs::write(&output.main_typ_path, &output.typ_content);

        let version_opt = {
            let mut overlay = self.state.tinymist_overlay.write().await;
            if let TinymistOverlay::Active { next_version } = &mut *overlay {
                let v = *next_version;
                *next_version += 1;
                Some(v)
            } else {
                None
            }
        };

        if let (Some(v), Ok(typ_uri)) = (version_opt, Url::from_file_path(&output.main_typ_path)) {
            let tinymist_guard = self.state.tinymist.read().await;
            if let Some(proxy) = tinymist_guard.as_ref() {
                let _ = proxy
                    .send_notification(
                        lsp::DID_CHANGE,
                        serde_json::json!({
                            "textDocument": { "uri": typ_uri, "version": v },
                            "contentChanges": [{ "text": &output.typ_content }],
                        }),
                    )
                    .await;
            }
        }
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
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);

        // Lazy-init: create an ExecutorManager for this URI if one doesn't
        // exist yet (hover/completion in R/Python chunks require it).
        {
            let mut executors = self.state.executors.write().await;
            executors
                .entry(uri.clone())
                .or_insert_with(|| ExecutorManager::new(cache_dir.clone()));
        }

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
