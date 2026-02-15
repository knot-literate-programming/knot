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
mod formatter;
mod handlers;
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
        if let Some(folders) = params.workspace_folders {
            if !folders.is_empty() {
                let mut root = self.root_uri.write().await;
                *root = Some(folders[0].uri.clone());
            }
        }

        // Read binary paths sent by the client (VS Code extension).
        if let Some(opts) = params.initialization_options {
            if let Some(air) = opts.get("airPath").and_then(|v| v.as_str()) {
                if !air.is_empty() && air != "air" {
                    *self.state.air_path_override.write().await = Some(PathBuf::from(air));
                }
            }
            if let Some(ruff) = opts.get("ruffPath").and_then(|v| v.as_str()) {
                if !ruff.is_empty() && ruff != "ruff" {
                    *self.state.ruff_path_override.write().await = Some(PathBuf::from(ruff));
                }
            }
            if let Some(tinymist) = opts.get("tinymistPath").and_then(|v| v.as_str()) {
                if !tinymist.is_empty() {
                    *self.state.tinymist_path_override.write().await =
                        Some(PathBuf::from(tinymist));
                }
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
            Some(formatter::CodeFormatter::new(air_path, ruff_path));
        // Spawn tinymist initialization in background to avoid blocking the LSP message loop
        let this = self.clone_for_task();
        let root_uri = self.root_uri.read().await.clone();
        let tinymist_path = self.state.tinymist_path_override.read().await.clone();
        tokio::spawn(async move {
            if let Ok((proxy, mut notification_rx)) =
                TinymistProxy::spawn(root_uri, tinymist_path).await
            {
                *this.state.tinymist.write().await = Some(proxy);
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
        self.forward_to_tinymist("textDocument/didOpen", &uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.first() {
            let text = change.text.clone();
            self.update_document(&uri, &text).await;
            self.publish_combined_diagnostics(&uri).await;
            self.forward_to_tinymist("textDocument/didChange", &uri)
                .await;
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
        self.forward_to_tinymist("textDocument/didSave", &uri).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        handle_completion(&self.state, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        handle_hover(&self.state, params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        handle_formatting(&self.state, params).await
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
        if params.command == "knot.cleanProject" {
            if let Some(arg) = params.arguments.first() {
                let uri_str = arg.as_str().map(|s| s.to_string());
                if let Some(s) = uri_str {
                    if let Ok(uri) = Url::parse(&s).or_else(|_| Url::from_file_path(&s)) {
                        if let Ok(path) = uri.to_file_path() {
                            let _ = knot_core::clean_project(Some(&path));
                            return Ok(Some(serde_json::json!({"status": "success"})));
                        }
                    }
                }
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
            state: ServerState {
                documents: self.state.documents.clone(),
                tinymist: self.state.tinymist.clone(),
                executors: self.state.executors.clone(),
                formatter: self.state.formatter.clone(),
                air_path_override: self.state.air_path_override.clone(),
                ruff_path_override: self.state.ruff_path_override.clone(),
                tinymist_path_override: self.state.tinymist_path_override.clone(),
                loaded_snapshot_hash: self.state.loaded_snapshot_hash.clone(),
            },
            root_uri: self.root_uri.clone(),
        })
    }

    fn resolve_virtual_uri(&self, uri: &Url) -> Url {
        if uri.scheme() == "knot-virtual" {
            let mut original_uri = uri.clone();
            if original_uri.set_scheme("file").is_ok() {
                let path = original_uri.path().to_string();
                if path.ends_with(".typ") {
                    let new_path = path.trim_end_matches(".typ").to_string();
                    original_uri.set_path(&new_path);
                }
                return original_uri;
            }
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
                    knot_diagnostics,
                    tinymist_diagnostics: Vec::new(),
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

        let actual_method = if !is_opened {
            "textDocument/didOpen"
        } else {
            method
        };

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
                        "textDocument/documentSymbol",
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
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);
        if let Ok(cache) = Cache::new(cache_dir.clone()) {
            if let Some(last_chunk) = cache
                .metadata
                .chunks
                .iter()
                .filter(|c| c.language == "r" && cache.has_snapshot(&c.hash, "RData"))
                .max_by_key(|c| c.index)
            {
                let reload_key = format!("{}::r", uri);
                if self
                    .state
                    .loaded_snapshot_hash
                    .read()
                    .await
                    .get(&reload_key)
                    != Some(&last_chunk.hash)
                {
                    if let Some(manager) = self.state.executors.write().await.get_mut(uri) {
                        if let Ok(executor) = manager.get_executor("r") {
                            if executor
                                .load_session(&cache.get_snapshot_path(&last_chunk.hash, "RData"))
                                .is_ok()
                            {
                                self.state
                                    .loaded_snapshot_hash
                                    .write()
                                    .await
                                    .insert(reload_key, last_chunk.hash.clone());
                                self.client
                                    .log_message(
                                        MessageType::INFO,
                                        format!("Synced R session (chunk {})", last_chunk.index),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            if let Some(last_chunk) = cache
                .metadata
                .chunks
                .iter()
                .filter(|c| c.language == "python" && cache.has_snapshot(&c.hash, "pkl"))
                .max_by_key(|c| c.index)
            {
                let reload_key = format!("{}::python", uri);
                if self
                    .state
                    .loaded_snapshot_hash
                    .read()
                    .await
                    .get(&reload_key)
                    != Some(&last_chunk.hash)
                {
                    if let Some(manager) = self.state.executors.write().await.get_mut(uri) {
                        if let Ok(executor) = manager.get_executor("python") {
                            if executor
                                .load_session(&cache.get_snapshot_path(&last_chunk.hash, "pkl"))
                                .is_ok()
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
                                            "Synced Python session (chunk {})",
                                            last_chunk.index
                                        ),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_tinymist_notification(&self, msg: serde_json::Value) {
        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
            if method == "textDocument/publishDiagnostics" {
                if let Some(params) = msg.get("params") {
                    if let (Some(uri_str), Some(diagnostics_val)) = (
                        params.get("uri").and_then(|u| u.as_str()),
                        params.get("diagnostics"),
                    ) {
                        if let (Ok(virtual_uri), Ok(mut diagnostics)) = (
                            Url::parse(uri_str),
                            serde_json::from_value::<Vec<Diagnostic>>(diagnostics_val.clone()),
                        ) {
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
                }
            }
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
