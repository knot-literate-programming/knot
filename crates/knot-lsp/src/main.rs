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
mod position_mapper;
mod proxy;
mod symbols;
mod transform;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use formatter::AirFormatter;
use knot_core::Document;
use position_mapper::PositionMapper;
use proxy::TinymistProxy;
use transform::transform_to_placeholder;

struct KnotLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
    mappers: Arc<RwLock<HashMap<Url, PositionMapper>>>,
    knot_diagnostics_cache: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
    opened_in_tinymist: Arc<RwLock<HashMap<Url, bool>>>,
    formatter: Option<AirFormatter>,
    tinymist: Arc<RwLock<Option<TinymistProxy>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for KnotLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
                    ]),
                    ..Default::default()
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: None,
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Knot LSP server initialized")
            .await;

        // Try to spawn tinymist subprocess
        match TinymistProxy::spawn().await {
            Ok((proxy, mut notification_rx)) => {
                self.client
                    .log_message(MessageType::INFO, "Tinymist proxy spawned successfully")
                    .await;
                *self.tinymist.write().await = Some(proxy);

                // Spawn a task to handle notifications from tinymist
                let client = self.client.clone();
                let mappers = self.mappers.clone();
                let knot_diagnostics_cache = self.knot_diagnostics_cache.clone();

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
        if let Some(mut proxy) = self.tinymist.write().await.take() {
            let _ = proxy.shutdown().await;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, format!("File opened: {}", params.text_document.uri))
            .await;

        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        // Store document text
        self.documents.write().await.insert(uri.clone(), text.clone());

        // Trigger diagnostics on file open
        self.on_change(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Get the full document text from the first change (FULL sync mode)
        if let Some(change) = params.content_changes.first() {
            let uri = params.text_document.uri.clone();
            let text = change.text.clone();

            // Update stored document text
            self.documents.write().await.insert(uri.clone(), text.clone());

            self.on_change(uri, text).await;
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
        let documents = self.documents.read().await;
        let text = match documents.get(&params.text_document.uri) {
            Some(text) => text,
            None => return Ok(None),
        };

        // Generate symbols
        let symbols = symbols::get_document_symbols(text);

        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn hover(&self, _params: HoverParams) -> Result<Option<Hover>> {
        // TODO: Implement hover information
        Ok(None)
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        // TODO: Implement completion
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        // Check if formatter is available
        let formatter = match &self.formatter {
            Some(f) => f,
            None => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        "Air formatter not available. Install from: https://posit-dev.github.io/air/",
                    )
                    .await;
                return Ok(None);
            }
        };

        // Get document text
        let documents = self.documents.read().await;
        let text = match documents.get(&params.text_document.uri) {
            Some(text) => text.clone(),
            None => return Ok(None),
        };
        drop(documents);

        // Parse document
        let doc = match Document::parse(text) {
            Ok(doc) => doc,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to parse document: {}", e))
                    .await;
                return Ok(None);
            }
        };

        // Format each R chunk
        let mut edits = Vec::new();
        for chunk in &doc.chunks {
            if chunk.language == "r" {
                match formatter.format_r_code(&chunk.code).await {
                    Ok(formatted) => {
                        // Only create edit if code changed
                        if formatted.trim() != chunk.code.trim() {
                            edits.push(TextEdit {
                                range: Range {
                                    start: Position {
                                        line: chunk.code_range.start.line as u32,
                                        character: chunk.code_range.start.column as u32,
                                    },
                                    end: Position {
                                        line: chunk.code_range.end.line as u32,
                                        character: chunk.code_range.end.column as u32,
                                    },
                                },
                                new_text: formatted,
                            });
                        }
                    }
                    Err(e) => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!("Failed to format chunk '{}': {}", chunk.name.as_ref().unwrap_or(&"unnamed".to_string()), e),
                            )
                            .await;
                    }
                }
            }
        }

        Ok(if edits.is_empty() { None } else { Some(edits) })
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        // Only format on newline
        if params.ch != "\n" {
            return Ok(None);
        }

        // Check if formatter is available
        let formatter = match &self.formatter {
            Some(f) => f,
            None => return Ok(None),
        };

        // Get document text
        let documents = self.documents.read().await;
        let text = match documents.get(&params.text_document_position.text_document.uri) {
            Some(text) => text.clone(),
            None => return Ok(None),
        };
        drop(documents);

        // Parse document
        let doc = match Document::parse(text) {
            Ok(doc) => doc,
            Err(_) => return Ok(None),
        };

        // Find the chunk containing the cursor position
        let cursor_line = params.text_document_position.position.line as usize;
        let current_chunk = doc.chunks.iter().find(|chunk| {
            chunk.language == "r"
                && chunk.range.start.line <= cursor_line
                && chunk.range.end.line >= cursor_line
        });

        // Format only the current chunk
        if let Some(chunk) = current_chunk {
            match formatter.format_r_code(&chunk.code).await {
                Ok(formatted) => {
                    if formatted.trim() != chunk.code.trim() {
                        return Ok(Some(vec![TextEdit {
                            range: Range {
                                start: Position {
                                    line: chunk.code_range.start.line as u32,
                                    character: chunk.code_range.start.column as u32,
                                },
                                end: Position {
                                    line: chunk.code_range.end.line as u32,
                                    character: chunk.code_range.end.column as u32,
                                },
                            },
                            new_text: formatted,
                        }]));
                    }
                }
                Err(_) => {
                    // Silently ignore formatting errors during on-type formatting
                }
            }
        }

        Ok(None)
    }
}

impl KnotLanguageServer {
    async fn on_change(&self, uri: Url, text: String) {
        // 1. Get knot-specific diagnostics (R chunks parsing)
        let knot_diagnostics = diagnostics::get_diagnostics(&text);

        // 2. Cache knot diagnostics
        self.knot_diagnostics_cache
            .write()
            .await
            .insert(uri.clone(), knot_diagnostics.clone());

        // 3. Publish knot diagnostics immediately (fast feedback)
        self.client
            .publish_diagnostics(uri.clone(), knot_diagnostics, None)
            .await;

        // 4. Transform .knot to .typ placeholder for tinymist
        let typ_content = transform_to_placeholder(&text);
        
        // 5. Create and store PositionMapper
        let mapper = PositionMapper::new(&text, &typ_content);
        self.mappers.write().await.insert(uri.clone(), mapper);

        // 6. Forward to tinymist subprocess
        self.send_to_tinymist(uri, typ_content).await;
    }

    async fn send_to_tinymist(&self, uri: Url, content: String) {
        let mut tinymist_guard = self.tinymist.write().await;
        if let Some(proxy) = tinymist_guard.as_mut() {
            let mut opened_map = self.opened_in_tinymist.write().await;
            let is_opened = *opened_map.get(&uri).unwrap_or(&false);

            let method = if is_opened {
                "textDocument/didChange"
            } else {
                "textDocument/didOpen"
            };

            let params = if is_opened {
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "version": 1, // We should track version but static 1 works for simple cases
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
                        "version": 1,
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
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Try to initialize Air formatter
    let formatter = match AirFormatter::new() {
        Ok(f) => {
            eprintln!("Air formatter initialized at: {:?}", f.path());
            Some(f)
        }
        Err(e) => {
            eprintln!("Air formatter not available: {}", e);
            eprintln!("R code formatting will be disabled");
            None
        }
    };

    let (service, socket) = LspService::new(|client| KnotLanguageServer {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
        mappers: Arc::new(RwLock::new(HashMap::new())),
        knot_diagnostics_cache: Arc::new(RwLock::new(HashMap::new())),
        opened_in_tinymist: Arc::new(RwLock::new(HashMap::new())),
        formatter,
        tinymist: Arc::new(RwLock::new(None)),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
