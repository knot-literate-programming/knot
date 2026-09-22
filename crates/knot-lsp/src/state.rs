use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use knot_core::CodeFormatter;
use knot_core::executors::ExecutorManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::{Diagnostic, Url};

/// Version counter for an opened generated Typst overlay. An absent map entry
/// means didOpen has not been sent; each project has its own entry.
pub enum TinymistOverlay {
    /// Next didChange version (didOpen uses 1).
    Active { next_version: u64 },
}

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
    /// Current version of the virtual Typst document in Tinymist
    pub virtual_version: i32,
    /// Knot-specific diagnostics
    pub knot_diagnostics: Vec<Diagnostic>,
    /// Tinymist-specific diagnostics (mapped to Knot positions)
    pub tinymist_diagnostics: Vec<Diagnostic>,
    /// Whether we have already notified the user about a formatting failure for this document
    pub formatting_error_notified: bool,
}

/// Centralized state for the Knot Language Server
///
/// Cloning shares all state with background tasks.
///
/// Acquire a project publication gate before individual state locks. Preview
/// startup takes its project startup lock before that gate. Never acquire a gate
/// while holding a document, overlay, proxy, or executor lock. Release each state
/// lock before acquiring another; clone the proxy before awaiting its I/O.
#[derive(Clone)]
pub struct ServerState {
    /// Per-document state
    pub documents: Arc<RwLock<HashMap<Url, DocumentState>>>,

    /// Global services and shared resources
    pub tinymist: Arc<RwLock<Option<TinymistProxy>>>,
    pub executors: Arc<RwLock<HashMap<Url, ExecutorManager>>>,
    pub formatter: Arc<RwLock<Option<CodeFormatter>>>,

    /// Active preview task managed by our tinymist subprocess.
    /// Stores `(task_id, static_server_port)` once `knot/startPreview` succeeds.
    pub preview_info: Arc<RwLock<HashMap<PathBuf, (String, u16)>>>,

    /// In-memory overlay state for `main.typ` in Tinymist.
    /// See [`TinymistOverlay`] for the state machine.
    pub tinymist_overlay: Arc<RwLock<HashMap<PathBuf, TinymistOverlay>>>,

    /// Per-project request ordering and publication gates.
    pub compilations: Arc<crate::compilation::Projects>,

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
            preview_info: Arc::new(RwLock::new(HashMap::new())),
            tinymist_overlay: Arc::new(RwLock::new(HashMap::new())),
            compilations: Arc::new(crate::compilation::Projects::default()),
            air_path_override: Arc::new(RwLock::new(None)),
            ruff_path_override: Arc::new(RwLock::new(None)),
            tinymist_path_override: Arc::new(RwLock::new(None)),
            loaded_snapshot_hash: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
