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
use crate::{FormatChunkParams, KnotLanguageServer};

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
