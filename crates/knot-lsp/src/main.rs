use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::get_cache_dir;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod diagnostics;
mod handlers;
mod lsp_methods;
mod path_resolver;
mod position_mapper;
mod proxy;
mod state;
mod symbols;
mod transform;

use diagnostics::get_diagnostics;
use handlers::completion::handle_completion;
use handlers::formatting::{handle_format_chunk, handle_formatting};
use handlers::hover::handle_hover;
use lsp_methods::text_document as lsp;
use lsp_methods::window as win;
use position_mapper::PositionMapper;
use proxy::TinymistProxy;
use state::ServerState;
use symbols::get_document_symbols;
use transform::transform_to_typst;

#[derive(Debug, Deserialize)]
struct FormatChunkParams {
    uri: Url,
    position: Position,
}

struct KnotLanguageServer {
    client: Client,
    state: ServerState,
    root_uri: Arc<RwLock<Option<Url>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for KnotLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(folders) = params.workspace_folders
            && !folders.is_empty()
        {
            let mut root = self.root_uri.write().await;
            *root = Some(folders[0].uri.clone());
        }

        // Read binary paths sent by the client (VS Code extension).
        if let Some(opts) = params.initialization_options {
            if let Some(air) = opts.get("airPath").and_then(|v| v.as_str())
                && !air.is_empty()
                && air != "air"
            {
                *self.state.air_path_override.write().await = Some(PathBuf::from(air));
            }
            if let Some(ruff) = opts.get("ruffPath").and_then(|v| v.as_str())
                && !ruff.is_empty()
                && ruff != "ruff"
            {
                *self.state.ruff_path_override.write().await = Some(PathBuf::from(ruff));
            }
            if let Some(tinymist) = opts.get("tinymistPath").and_then(|v| v.as_str())
                && !tinymist.is_empty()
            {
                *self.state.tinymist_path_override.write().await = Some(PathBuf::from(tinymist));
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "$".to_string(),
                        ":".to_string(),
                        "|".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![], // Commands handled via custom LSP requests, not registered here
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Knot LSP server initialized")
            .await;
        let air_path = self.state.air_path_override.read().await.clone();
        let ruff_path = self.state.ruff_path_override.read().await.clone();
        *self.state.formatter.write().await =
            Some(knot_core::CodeFormatter::new(air_path, ruff_path));
        // Spawn tinymist initialization in background to avoid blocking the LSP message loop
        let this = self.clone_for_task();
        let root_uri = self.root_uri.read().await.clone();
        let tinymist_path = self.state.tinymist_path_override.read().await.clone();
        tokio::spawn(async move {
            if let Ok((proxy, mut notification_rx)) =
                TinymistProxy::spawn(root_uri, tinymist_path).await
            {
                *this.state.tinymist.write().await = Some(proxy);

                // VS Code may have sent didOpen for documents before Tinymist was
                // ready. Re-sync any document not yet forwarded so Tinymist has the
                // virtual content and can answer hover / diagnostic requests.
                let pending: Vec<Url> = this
                    .state
                    .documents
                    .read()
                    .await
                    .iter()
                    .filter(|(_, doc)| !doc.opened_in_tinymist)
                    .map(|(uri, _)| uri.clone())
                    .collect();
                for uri in pending {
                    this.forward_to_tinymist(lsp::DID_OPEN, &uri).await;
                }

                while let Some(msg) = notification_rx.recv().await {
                    this.handle_tinymist_notification(msg).await;
                }
            }
        });
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(proxy) = self.state.tinymist.write().await.as_mut() {
            let _ = proxy.shutdown().await;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.update_document(&uri, &text).await;
        self.publish_combined_diagnostics(&uri).await;
        self.sync_with_cache(&uri).await;
        self.forward_to_tinymist(lsp::DID_OPEN, &uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.first() {
            let text = change.text.clone();
            self.update_document(&uri, &text).await;
            self.publish_combined_diagnostics(&uri).await;
            self.forward_to_tinymist(lsp::DID_CHANGE, &uri).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let this = self.clone_for_task();
        let uri_for_cache = uri.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            this.sync_with_cache(&uri_for_cache).await;
        });
        self.forward_to_tinymist(lsp::DID_SAVE, &uri).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        handle_completion(&self.state, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        handle_hover(&self.state, params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        handle_formatting(&self.state, &self.client, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.state.documents.read().await;
        let text = match docs.get(&uri) {
            Some(doc) => &doc.text,
            None => return Ok(None),
        };
        if let Some(symbols) = get_document_symbols(text) {
            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }
        Ok(None)
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command == "knot.cleanProject"
            && let Some(arg) = params.arguments.first()
        {
            let uri_str = arg.as_str().map(|s| s.to_string());
            if let Some(s) = uri_str
                && let Ok(uri) = Url::parse(&s).or_else(|_| Url::from_file_path(&s))
                && let Ok(path) = uri.to_file_path()
            {
                let _ = knot_core::clean_project(Some(&path));
                return Ok(Some(serde_json::json!({"status": "success"})));
            }
        }
        Ok(None)
    }
}

impl KnotLanguageServer {
    async fn handle_custom_format_chunk(
        &self,
        params: FormatChunkParams,
    ) -> Result<serde_json::Value> {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "LSP: Received request knot/formatChunk at line {}",
                    params.position.line
                ),
            )
            .await;
        match handle_format_chunk(&self.state, &params.uri, params.position).await {
            Ok(Some(edit)) => {
                let _ = self.client.apply_edit(edit).await;
                Ok(serde_json::json!({"status": "success"}))
            }
            Ok(None) => Ok(serde_json::json!({"status": "no_changes"})),
            Err(_) => Err(tower_lsp::jsonrpc::Error::internal_error()),
        }
    }

    fn clone_for_task(&self) -> Arc<Self> {
        Arc::new(KnotLanguageServer {
            client: self.client.clone(),
            state: self.state.clone(),
            root_uri: self.root_uri.clone(),
        })
    }

    fn resolve_virtual_uri(&self, uri: &Url) -> Url {
        // Tinymist normalizes our knot-virtual:// URIs to file:// on receipt,
        // so publishDiagnostics comes back as file://...foo.knot.typ.
        // We handle both cases: knot-virtual:// (if ever preserved) and
        // file:// with the .knot.typ double extension we create in to_virtual_uri.
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

    async fn update_document(&self, uri: &Url, text: &str) {
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

    async fn publish_combined_diagnostics(&self, uri: &Url) {
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

    async fn forward_to_tinymist(&self, method: &str, uri: &Url) {
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

    async fn sync_with_cache(&self, uri: &Url) {
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        let project_root = match Config::find_project_root(&path) {
            Ok(root) => root,
            Err(_) => return,
        };
        let cache_dir = get_cache_dir(&project_root);
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

    async fn handle_tinymist_notification(&self, msg: serde_json::Value) {
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            return;
        };

        match method {
            lsp::PUBLISH_DIAGNOSTICS => {
                if let Some(params) = msg.get("params")
                    && let (Some(uri_str), Some(diagnostics_val)) = (
                        params.get("uri").and_then(|u| u.as_str()),
                        params.get("diagnostics"),
                    )
                    && let (Ok(virtual_uri), Ok(mut diagnostics)) = (
                        Url::parse(uri_str),
                        serde_json::from_value::<Vec<Diagnostic>>(diagnostics_val.clone()),
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

            win::SHOW_MESSAGE | win::LOG_MESSAGE => {
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

            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::build(|client| KnotLanguageServer {
        client,
        state: ServerState::new(),
        root_uri: Arc::new(RwLock::new(None)),
    })
    .custom_method(
        "knot/formatChunk",
        KnotLanguageServer::handle_custom_format_chunk,
    )
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
