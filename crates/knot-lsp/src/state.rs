use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Diagnostic, Url};
use std::path::PathBuf;
use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use crate::formatter::AirFormatter;
use knot_core::executors::r::RExecutor;

/// Centralized state for the Knot Language Server
pub struct ServerState {
    /// Stored document text by URI
    pub documents: Arc<RwLock<HashMap<Url, String>>>,
    /// Mappers for position translation (Knot <-> Typst)
    pub mappers: Arc<RwLock<HashMap<Url, PositionMapper>>>,
    /// Cached Knot-specific diagnostics
    pub knot_diagnostics_cache: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
    /// Tracking which files are opened in the tinymist proxy
    pub opened_in_tinymist: Arc<RwLock<HashMap<Url, bool>>>,
    /// Tracking document versions for tinymist synchronization
    pub document_versions: Arc<RwLock<HashMap<Url, i32>>>,
    /// The R code formatter (Air)
    pub formatter: Arc<RwLock<Option<AirFormatter>>>,
    /// Live R executors for each document (for completion/hover)
    pub r_executors: Arc<RwLock<HashMap<Url, RExecutor>>>,
    /// Path overrides from client initialization
    pub air_path_override: Arc<RwLock<Option<PathBuf>>>,
    pub tinymist_path_override: Arc<RwLock<Option<PathBuf>>>,
    /// The proxy to the tinymist LSP subprocess
    pub tinymist: Arc<RwLock<Option<TinymistProxy>>>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            mappers: Arc::new(RwLock::new(HashMap::new())),
            knot_diagnostics_cache: Arc::new(RwLock::new(HashMap::new())),
            opened_in_tinymist: Arc::new(RwLock::new(HashMap::new())),
            document_versions: Arc::new(RwLock::new(HashMap::new())),
            formatter: Arc::new(RwLock::new(None)),
            r_executors: Arc::new(RwLock::new(HashMap::new())),
            air_path_override: Arc::new(RwLock::new(None)),
            tinymist_path_override: Arc::new(RwLock::new(None)),
            tinymist: Arc::new(RwLock::new(None)),
        }
    }
}
