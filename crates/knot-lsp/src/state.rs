use crate::formatter::CodeFormatter;
use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use knot_core::executors::ExecutorManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Diagnostic, Url};

/// State specific to a single opened document
pub struct DocumentState {
    /// Current text content
    pub text: String,
    /// Current version for LSP synchronization
    pub version: i32,
    /// Position mapper for this version
    pub mapper: PositionMapper,
    /// Whether this document is successfully opened in the Tinymist proxy
    pub opened_in_tinymist: bool,
    /// Knot-specific diagnostics
    pub knot_diagnostics: Vec<Diagnostic>,
    /// Tinymist-specific diagnostics (mapped to Knot positions)
    pub tinymist_diagnostics: Vec<Diagnostic>,
}

/// Centralized state for the Knot Language Server
pub struct ServerState {
    /// Per-document state
    pub documents: Arc<RwLock<HashMap<Url, DocumentState>>>,

    /// Global services and shared resources
    pub tinymist: Arc<RwLock<Option<TinymistProxy>>>,
    pub executors: Arc<RwLock<HashMap<Url, ExecutorManager>>>,
    pub formatter: Arc<RwLock<Option<CodeFormatter>>>,

    /// Global configuration and caches
    pub air_path_override: Arc<RwLock<Option<PathBuf>>>,
    pub ruff_path_override: Arc<RwLock<Option<PathBuf>>>,
    pub tinymist_path_override: Arc<RwLock<Option<PathBuf>>>,
    pub loaded_snapshot_hash: Arc<RwLock<HashMap<String, String>>>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            tinymist: Arc::new(RwLock::new(None)),
            executors: Arc::new(RwLock::new(HashMap::new())),
            formatter: Arc::new(RwLock::new(None)),
            air_path_override: Arc::new(RwLock::new(None)),
            ruff_path_override: Arc::new(RwLock::new(None)),
            tinymist_path_override: Arc::new(RwLock::new(None)),
            loaded_snapshot_hash: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
