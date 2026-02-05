// Knot Language Server Protocol Implementation
//
// Provides IDE support for .knot files including:
// - Diagnostics (parsing errors, invalid options)
// - Document symbols (chunk listing)
// - Hover information (chunk metadata)
// - Completion (chunk options and names)
// - R code formatting with Air

mod diagnostics;
mod formatter;
mod handlers;
mod path_resolver;
mod position_mapper;
mod proxy;
mod state;
mod symbols;
mod transform;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use formatter::AirFormatter;
use position_mapper::PositionMapper;
use proxy::TinymistProxy;
use state::ServerState;
use transform::transform_to_typst;
use knot_core::executors::ExecutorManager;
 // Traits used for dynamic dispatch // Ensure traits are imported

struct KnotLanguageServer {
    client: Client,
    state: ServerState,
    root_uri: Arc<RwLock<Option<Url>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for KnotLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(uri) = params.root_uri {
            *self.root_uri.write().await = Some(uri);
        } else if let Some(folders) = params.workspace_folders {
            if let Some(first) = folders.first() {
                *self.root_uri.write().await = Some(first.uri.clone());
            }
        }

        // Handle initialization options
        if let Some(options) = params.initialization_options {
            if let Some(air_path) = options.get("airPath").and_then(|v| v.as_str()) {
                *self.state.air_path_override.write().await = Some(PathBuf::from(air_path));
            }
            if let Some(tinymist_path) = options.get("tinymistPath").and_then(|v| v.as_str()) {
                *self.state.tinymist_path_override.write().await = Some(PathBuf::from(tinymist_path));
            }
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "knot-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("knot".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "#".to_string(),
                        "|".to_string(),
                        ":".to_string(),
                        "$".to_string(), // Trigger completion for df$ (column names)
                    ]),
                    ..Default::default()
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["knot.cleanProject".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        if params.command == "knot.cleanProject" {
            // Get the file URI from arguments if present
            let start_path = if let Some(first) = params.arguments.first() {
                if let Some(uri_str) = first.as_str() {
                    if let Ok(uri) = Url::parse(uri_str) {
                        uri.to_file_path().ok()
                    } else { None }
                } else { None }
            } else { None };

            // Fallback to workspace root
            let root_path = if start_path.is_some() {
                start_path
            } else {
                let guard = self.root_uri.read().await;
                guard.as_ref().and_then(|uri| uri.to_file_path().ok())
            };

            if let Some(path) = &root_path {
                self.client.log_message(MessageType::INFO, format!("LSP: Cleaning project starting from {:?}", path)).await;
            } else {
                self.client.log_message(MessageType::INFO, "LSP: Cleaning project (unknown root)").await;
            }
            
            match knot_core::clean_project(root_path.as_deref()) {
                Ok(_) => {
                    self.client.show_message(MessageType::INFO, "Project cleaned successfully!").await;
                }
                Err(e) => {
                    self.client.show_message(MessageType::ERROR, format!("Failed to clean project: {}", e)).await;
                }
            }
        }
        Ok(None)
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Knot LSP server initialized")
            .await;

        let root_uri = self.root_uri.read().await.clone();
        let tinymist_override = self.state.tinymist_path_override.read().await.clone();
        let air_override = self.state.air_path_override.read().await.clone();

        // Initialize Air formatter if not already done (with possible override)
        match AirFormatter::new(air_override) {
            Ok(f) => {
                self.client.log_message(MessageType::INFO, format!("Air formatter initialized at: {:?}", f.path())).await;
                *self.state.formatter.write().await = Some(f);
            }
            Err(e) => {
                self.client.log_message(MessageType::WARNING, format!("Air formatter not available: {}", e)).await;
            }
        }

        // Try to spawn tinymist subprocess
        match TinymistProxy::spawn(root_uri, tinymist_override).await {
            Ok((proxy, mut notification_rx)) => {
                self.client
                    .log_message(MessageType::INFO, "Tinymist proxy spawned successfully")
                    .await;
                *self.state.tinymist.write().await = Some(proxy);

                // Spawn a task to handle notifications from tinymist
                let client = self.client.clone();
                let mappers = self.state.mappers.clone();
                let knot_diagnostics_cache = self.state.knot_diagnostics_cache.clone();

                tokio::spawn(async move {
                    while let Some(msg) = notification_rx.recv().await {
                        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
                            if method == "textDocument/publishDiagnostics" {
                                if let Some(params) = msg.get("params") {
                                    if let Ok(diag_params) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone()) {
                                        // Retrieve mapper and knot diagnostics
                                        let uri = diag_params.uri.clone();
                                        
                                        // Get cached knot diagnostics
                                        let mut merged_diagnostics = {
                                            let cache = knot_diagnostics_cache.read().await;
                                            cache.get(&uri).cloned().unwrap_or_default()
                                        };

                                        // Map tinymist diagnostics
                                        if let Some(mapper) = mappers.read().await.get(&uri) {
                                            for mut diag in diag_params.diagnostics {
                                                if let Some(start) = mapper.typ_to_knot_position(diag.range.start) {
                                                    if let Some(end) = mapper.typ_to_knot_position(diag.range.end) {
                                                        diag.range.start = start;
                                                        diag.range.end = end;
                                                        diag.source = Some("typst".to_string());
                                                        merged_diagnostics.push(diag);
                                                    }
                                                }
                                            }
                                        }

                                        // Publish merged diagnostics
                                        client.publish_diagnostics(uri, merged_diagnostics, None).await;
                                    }
                                }
                            }
                        }
                    }
                });
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("Failed to spawn tinymist: {}. Typst features will be limited.", e),
                    )
                    .await;
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        // Shutdown tinymist if it's running
        if let Some(mut proxy) = self.state.tinymist.write().await.take() {
            let _ = proxy.shutdown().await;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        // Store document text
        self.state.documents.write().await.insert(uri.clone(), text.clone());

        // Initialize ExecutorManager for this document
        {
            let mut managers = self.state.executors.write().await;
            if !managers.contains_key(&uri) {
                // Create a temp dir for this LSP session
                let temp_dir = std::env::temp_dir().join(format!("knot_lsp_{}", uuid::Uuid::new_v4()));
                
                // Initialize ExecutorManager
                // Note: r_helper_path is None here, we might want to pass it if we can resolve it quickly
                // For now LSP uses a simpler initialization
                let manager = ExecutorManager::new(temp_dir, None);
                managers.insert(uri.clone(), manager);
                self.client.log_message(MessageType::INFO, "Initialized executor manager for document").await;
            }
        }

        // Sync with cache to restore previous session state (only on open, not on every change)
        self.sync_with_cache(&uri).await;

        // Trigger diagnostics on file open
        self.on_change(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Get the full document text from the first change (FULL sync mode)
        if let Some(change) = params.content_changes.first() {
            let uri = params.text_document.uri.clone();
            let text = change.text.clone();

            // Update stored document text
            self.state.documents.write().await.insert(uri.clone(), text.clone());

            self.on_change(uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.state.documents.write().await.remove(&uri);
        self.state.mappers.write().await.remove(&uri);
        
        // Remove and drop executors (terminates processes)
        if let Some(_manager) = self.state.executors.write().await.remove(&uri) {
            self.client.log_message(MessageType::INFO, "Closed execution session for document").await;
        }
    }

    async fn diagnostic(
        &self,
        _params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        // We handle diagnostics via push (publishDiagnostics)
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(
                RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items: vec![],
                    },
                },
            ),
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        // Get document text
        let documents = self.state.documents.read().await;
        let text = match documents.get(&params.text_document.uri) {
            Some(text) => text,
            None => return Ok(None),
        };

        // Generate symbols
        let symbols = symbols::get_document_symbols(text);

        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        handlers::hover::handle_hover(&self.state, params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        handlers::completion::handle_completion(&self.state, params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        handlers::formatting::handle_formatting(&self.state, params).await
    }

    async fn on_type_formatting(
        &self,
        _params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        Ok(None)
    }
}

impl KnotLanguageServer {
    async fn on_change(&self, uri: Url, text: String) {
        // 1. Get knot-specific diagnostics (R chunks parsing)
        let knot_diagnostics = diagnostics::get_diagnostics(&text);

        // 2. Cache knot diagnostics
        self.state.knot_diagnostics_cache
            .write()
            .await
            .insert(uri.clone(), knot_diagnostics.clone());

        // 3. Publish knot diagnostics immediately (fast feedback)
        self.client
            .publish_diagnostics(uri.clone(), knot_diagnostics, None)
            .await;

        // 4. Transform .knot to .typ placeholder for tinymist
        let typ_content = transform_to_typst(&text);

        // 5. Create and store PositionMapper
        let mapper = PositionMapper::new(&text, &typ_content);
        self.state.mappers.write().await.insert(uri.clone(), mapper);

        // 6. Forward to tinymist subprocess
        self.send_to_tinymist(uri, typ_content).await;
    }

    async fn send_to_tinymist(&self, uri: Url, content: String) {
        let mut tinymist_guard = self.state.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let mut opened_map = self.state.opened_in_tinymist.write().await;
            let mut versions_map = self.state.document_versions.write().await;
            
            let is_opened = *opened_map.get(&uri).unwrap_or(&false);
            let version = *versions_map.get(&uri).unwrap_or(&0) + 1;
            versions_map.insert(uri.clone(), version);

            let method = if is_opened {
                "textDocument/didChange"
            } else {
                "textDocument/didOpen"
            };

            let params = if is_opened {
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "version": version,
                    },
                    "contentChanges": [
                        { "text": content }
                    ]
                })
            } else {
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "typst",
                        "version": version,
                        "text": content
                    }
                })
            };

            // Send notification
            if let Err(e) = proxy.send_notification(method, params).await {
                self.client.log_message(MessageType::ERROR, format!("Failed to send to tinymist: {}", e)).await;
            } else if !is_opened {
                opened_map.insert(uri, true);
            }
        }
    }

    async fn sync_with_cache(&self, uri: &Url) {
        use knot_core::config::Config;
        use knot_core::get_cache_dir;
        use knot_core::cache::Cache;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        // 1. Find project root and cache dir
        let start_dir = path.parent().unwrap_or(Path::new("."));
        let (_config, project_root) = match Config::find_and_load(start_dir) {
            Ok(res) => res,
            Err(_) => return,
        };

        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);

        // 2. Load metadata to find the latest snapshot for R
        if let Ok(cache) = Cache::new(cache_dir.clone()) {
            // Find the last chunk that was executed in R
            let last_r_chunk = cache.metadata.chunks.iter()
                .filter(|c| {
                    // Check if chunk is R AND snapshot exists with .RData extension
                    c.language == "r" && cache.has_snapshot(&c.hash, "RData")
                })
                .max_by_key(|c| c.index);

            if let Some(last_chunk) = last_r_chunk {
                let snapshot_hash = &last_chunk.hash;

                // 3. Check if this snapshot is different from the last loaded one
                let should_reload = {
                    let loaded_hashes = self.state.loaded_snapshot_hash.read().await;
                    loaded_hashes.get(uri).map_or(true, |h| h != snapshot_hash)
                };

                if should_reload {
                    let snapshot_path = cache.get_snapshot_path(snapshot_hash, "RData");
                    if snapshot_path.exists() {
                        // 4. Load this snapshot into our live R session
                        let result = {
                            let mut managers = self.state.executors.write().await;
                            if let Some(manager) = managers.get_mut(uri) {
                                // We explicitly want to load the R session
                                if let Ok(executor) = manager.get_executor("r") {
                                    executor.load_session(&snapshot_path).map(|_| true)
                                } else {
                                    Ok(false)
                                }
                            } else {
                                Ok(false)
                            }
                        };

                        match result {
                            Ok(true) => {
                                // 5. Update the loaded hash
                                self.state.loaded_snapshot_hash.write().await.insert(uri.clone(), snapshot_hash.clone());
                                self.client.log_message(MessageType::INFO, format!("Synced R session with snapshot for chunk {} (hash: {})", last_chunk.index, &snapshot_hash[..8])).await;
                            }
                            Ok(false) => {}
                            Err(e) => {
                                self.client.log_message(MessageType::WARNING, format!("Failed to load R snapshot: {}", e)).await;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main]

async fn main() {

    let stdin = tokio::io::stdin();

    let stdout = tokio::io::stdout();



    let (service, socket) = LspService::new(|client| KnotLanguageServer {

        client,

        state: ServerState::new(),

        root_uri: Arc::new(RwLock::new(None)),

    });

    Server::new(stdin, stdout, socket).serve(service).await;

}
