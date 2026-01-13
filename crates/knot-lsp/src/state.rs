use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Diagnostic, Url};
use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use crate::formatter::AirFormatter;

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
    /// The R code formatter (Air)
    pub formatter: Option<AirFormatter>,
    /// The proxy to the tinymist LSP subprocess
    pub tinymist: Arc<RwLock<Option<TinymistProxy>>>,
}

impl ServerState {
    pub fn new(formatter: Option<AirFormatter>) -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            mappers: Arc::new(RwLock::new(HashMap::new())),
            knot_diagnostics_cache: Arc::new(RwLock::new(HashMap::new())),
            opened_in_tinymist: Arc::new(RwLock::new(HashMap::new())),
            formatter,
            tinymist: Arc::new(RwLock::new(None)),
        }
    }
}
