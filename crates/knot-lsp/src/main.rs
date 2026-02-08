use knot_core::cache::Cache;
use knot_core::config::Config;
use knot_core::get_cache_dir;
use serde::Deserialize;
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
use handlers::formatting::handle_formatting;
use handlers::hover::handle_hover;
use position_mapper::PositionMapper;
use proxy::TinymistProxy;
use state::ServerState;
use symbols::get_document_symbols;
use transform::transform_to_typst;

/// Robust VS Code URI representation
#[derive(Debug, Deserialize)]
struct VsCodeUri {
    external: Option<String>,
    path: Option<String>,
    #[serde(rename = "fsPath")]
    fs_path: Option<String>,
}

impl VsCodeUri {
    fn preferred_uri(&self) -> Option<String> {
        self.external
            .clone()
            .or_else(|| self.path.clone())
            .or_else(|| self.fs_path.clone())
    }
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

        if let Some(options) = params.initialization_options {
            if let Some(air_path) = options.get("airPath").and_then(|v| v.as_str()) {
                let mut p = self.state.air_path_override.write().await;
                *p = Some(air_path.into());
            }
            if let Some(tinymist_path) = options.get("tinymistPath").and_then(|v| v.as_str()) {
                let mut p = self.state.tinymist_path_override.write().await;
                *p = Some(tinymist_path.into());
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "$".to_string(),
                        ":".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(false)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["knot.cleanProject".to_string()],
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
        let formatter = formatter::AirFormatter::new(air_path);
        if let Ok(f) = formatter {
            let mut formatter_guard = self.state.formatter.write().await;
            *formatter_guard = Some(f);
        }

        let tinymist_path = self.state.tinymist_path_override.read().await.clone();
        let root_uri = self.root_uri.read().await.clone();
        match TinymistProxy::spawn(root_uri, tinymist_path).await {
            Ok((proxy, mut notification_rx)) => {
                let mut tinymist_guard = self.state.tinymist.write().await;
                *tinymist_guard = Some(proxy);
                self.client
                    .log_message(MessageType::INFO, "Tinymist proxy spawned successfully")
                    .await;

                // Spawn background task to handle notifications from tinymist
                let this = self.clone_for_task();
                tokio::spawn(async move {
                    while let Some(msg) = notification_rx.recv().await {
                        this.handle_tinymist_notification(msg).await;
                    }
                });
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to spawn tinymist: {}", e),
                    )
                    .await;
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        let mut tinymist_guard = self.state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let _ = proxy.shutdown().await;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        {
            let mut docs = self.state.documents.write().await;
            docs.insert(uri.clone(), text.clone());
        }

        self.update_mapper(&uri, &text).await;
        self.publish_diagnostics(&uri, &text).await;

        {
            let mut managers = self.state.executors.write().await;
            if !managers.contains_key(&uri) {
                let path = uri.to_file_path().unwrap_or_default();
                if let Ok(project_root) = Config::find_project_root(&path) {
                    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
                    let cache_dir = get_cache_dir(&project_root, file_stem);
                    managers.insert(
                        uri.clone(),
                        knot_core::executors::ExecutorManager::new(cache_dir),
                    );
                }
            }
        }

        self.sync_with_cache(&uri).await;
        self.forward_to_tinymist("textDocument/didOpen", &uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.first() {
            let text = change.text.clone();
            {
                let mut docs = self.state.documents.write().await;
                docs.insert(uri.clone(), text.clone());
            }
            self.update_mapper(&uri, &text).await;
            self.publish_diagnostics(&uri, &text).await;
            self.forward_to_tinymist("textDocument/didChange", &uri)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let this = self.clone_for_task();
        let uri_clone = uri.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            this.sync_with_cache(&uri_clone).await;
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
            Some(t) => t,
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
        self.client
            .log_message(
                MessageType::INFO,
                format!("LSP: Executing command: {}", params.command),
            )
            .await;

        match params.command.as_str() {
            "knot.cleanProject" => {
                if let Some(arg) = params.arguments.first() {
                    // 1. Try direct string
                    let mut uri_str = arg.as_str().map(|s| s.to_string());

                    // 2. Try Serde deserialization
                    if uri_str.is_none() {
                        if let Ok(vs_uri) = serde_json::from_value::<VsCodeUri>(arg.clone()) {
                            uri_str = vs_uri.preferred_uri();
                        }
                    }

                    if let Some(s) = uri_str {
                        self.client
                            .log_message(MessageType::INFO, format!("LSP: Target URI: {}", s))
                            .await;
                        // Handle both URI string and raw path
                        let url = Url::parse(&s).ok().or_else(|| Url::from_file_path(&s).ok());

                        if let Some(uri) = url {
                            if let Ok(path) = uri.to_file_path() {
                                if let Err(e) = knot_core::clean_project(Some(&path)) {
                                    self.client
                                        .log_message(
                                            MessageType::ERROR,
                                            format!("LSP: Clean failed: {}", e),
                                        )
                                        .await;
                                    return Err(tower_lsp::jsonrpc::Error::invalid_params(
                                        format!("Clean failed: {}", e),
                                    ));
                                }
                                self.client
                                    .log_message(
                                        MessageType::INFO,
                                        "LSP: Project cleaned successfully",
                                    )
                                    .await;
                                return Ok(Some(serde_json::json!({"status": "success"})));
                            }
                        }
                    }
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            "LSP: Could not extract valid path from arguments",
                        )
                        .await;
                }
            }
            _ => {}
        }
        Ok(None)
    }
}

impl KnotLanguageServer {
    fn clone_for_task(&self) -> Arc<Self> {
        Arc::new(KnotLanguageServer {
            client: self.client.clone(),
            state: ServerState {
                documents: self.state.documents.clone(),
                mappers: self.state.mappers.clone(),
                knot_diagnostics_cache: self.state.knot_diagnostics_cache.clone(),
                opened_in_tinymist: self.state.opened_in_tinymist.clone(),
                document_versions: self.state.document_versions.clone(),
                formatter: self.state.formatter.clone(),
                executors: self.state.executors.clone(),
                air_path_override: self.state.air_path_override.clone(),
                tinymist_path_override: self.state.tinymist_path_override.clone(),
                tinymist: self.state.tinymist.clone(),
                loaded_snapshot_hash: self.state.loaded_snapshot_hash.clone(),
            },
            root_uri: self.root_uri.clone(),
        })
    }

    async fn update_mapper(&self, uri: &Url, text: &str) {
        let typ_text = transform_to_typst(text);
        let mapper = PositionMapper::new(text, &typ_text);
        let mut mappers = self.state.mappers.write().await;
        mappers.insert(uri.clone(), mapper);
    }

    async fn publish_diagnostics(&self, uri: &Url, text: &str) {
        let diagnostics = get_diagnostics(text);
        let _ = self
            .client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    async fn forward_to_tinymist(&self, method: &str, uri: &Url) {
        let mut tinymist_guard = self.state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let content = {
                let docs = self.state.documents.read().await;
                docs.get(uri).cloned().unwrap_or_default()
            };

            let mut opened_map = self.state.opened_in_tinymist.write().await;
            let mut versions = self.state.document_versions.write().await;

            let is_opened = opened_map.get(uri).cloned().unwrap_or(false);
            let version = versions.get(uri).cloned().unwrap_or(0) + 1;
            versions.insert(uri.clone(), version);

            let params = if method == "textDocument/didOpen" || !is_opened {
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "typst",
                        "version": version,
                        "text": transform_to_typst(&content)
                    }
                })
            } else {
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "version": version
                    },
                    "contentChanges": [{
                        "text": transform_to_typst(&content)
                    }]
                })
            };

            let actual_method = if !is_opened {
                "textDocument/didOpen"
            } else {
                method
            };

            if let Err(e) = proxy.send_notification(actual_method, params).await {
                let _ = self.client.log_message(
                    MessageType::ERROR,
                    format!("Failed to send to tinymist: {}", e),
                ).await;
            } else if !is_opened {
                opened_map.insert(uri.clone(), true);
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
            let last_r_chunk = cache
                .metadata
                .chunks
                .iter()
                .filter(|c| c.language == "r" && cache.has_snapshot(&c.hash, "RData"))
                .max_by_key(|c| c.index);

            if let Some(last_chunk) = last_r_chunk {
                let snapshot_hash = &last_chunk.hash;
                let reload_key = format!("{}::r", uri);

                let should_reload = {
                    let loaded_hashes = self.state.loaded_snapshot_hash.read().await;
                    loaded_hashes.get(&reload_key) != Some(snapshot_hash)
                };

                if should_reload {
                    let snapshot_path = cache.get_snapshot_path(snapshot_hash, "RData");
                    let mut managers = self.state.executors.write().await;
                    if let Some(manager) = managers.get_mut(uri) {
                        if let Ok(executor) = manager.get_executor("r") {
                            if executor.load_session(&snapshot_path).is_ok() {
                                self.state
                                    .loaded_snapshot_hash
                                    .write()
                                    .await
                                    .insert(reload_key, snapshot_hash.clone());
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

            let last_py_chunk = cache
                .metadata
                .chunks
                .iter()
                .filter(|c| c.language == "python" && cache.has_snapshot(&c.hash, "pkl"))
                .max_by_key(|c| c.index);

            if let Some(last_chunk) = last_py_chunk {
                let snapshot_hash = &last_chunk.hash;
                let reload_key = format!("{}::python", uri);

                let should_reload = {
                    let loaded_hashes = self.state.loaded_snapshot_hash.read().await;
                    loaded_hashes.get(&reload_key) != Some(snapshot_hash)
                };

                if should_reload {
                    let snapshot_path = cache.get_snapshot_path(snapshot_hash, "pkl");
                    let mut managers = self.state.executors.write().await;
                    if let Some(manager) = managers.get_mut(uri) {
                        if let Ok(executor) = manager.get_executor("python") {
                            if executor.load_session(&snapshot_path).is_ok() {
                                self.state
                                    .loaded_snapshot_hash
                                    .write()
                                    .await
                                    .insert(reload_key, snapshot_hash.clone());
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

    /// Handle notifications coming from the Tinymist subprocess
    async fn handle_tinymist_notification(&self, msg: serde_json::Value) {
        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
            match method {
                "textDocument/publishDiagnostics" => {
                    // This is where diagnostics mapping logic will go.
                    // 1. Extract Typst diagnostics from msg["params"]
                    // 2. Map ranges (Typst -> Knot)
                    // 3. Publish to client
                    let _ = self.client.log_message(
                        MessageType::LOG,
                        "LSP: Received diagnostics from Tinymist (mapping logic needed)",
                    ).await;
                }
                _ => {
                    // Log other notifications for development/debugging
                    let _ = self.client.log_message(
                        MessageType::LOG,
                        format!("LSP: Received notification from Tinymist: {}", method),
                    ).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logger to stderr (important for LSP stability)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| KnotLanguageServer {
        client,
        state: ServerState::new(),
        root_uri: Arc::new(RwLock::new(None)),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
