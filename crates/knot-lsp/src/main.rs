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
mod server_impl;
mod state;
mod symbols;
mod transform;

use handlers::completion::handle_completion;
use handlers::formatting::handle_formatting;
use handlers::hover::handle_hover;
use lsp_methods::text_document as lsp;
use proxy::TinymistProxy;
use state::ServerState;
use symbols::get_document_symbols;

#[derive(Debug, Deserialize)]
struct FormatChunkParams {
    uri: Url,
    position: Position,
}

#[derive(Debug, Deserialize)]
struct SyncForwardParams {
    uri: Url,
    line: u32,
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
            // Refresh diagnostics as soon as the cache is updated by the compiler.
            // This picks up runtime errors and freeze violations written by `knot build`
            // or `knot watch` without waiting for the user to make an edit.
            this.refresh_diagnostics_on_cache_update(&uri_for_cache)
                .await;
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
    .custom_method("knot/syncForward", KnotLanguageServer::handle_sync_forward)
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
