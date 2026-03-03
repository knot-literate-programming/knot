use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use knot_core::CodeFormatter;
use knot_core::executors::ExecutorManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
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
/// All fields are `Arc<RwLock<…>>` so cloning shares the same underlying
/// data — it does NOT deep-copy the state.  This is intentional: clones are
/// used to hand the state to background tasks (`clone_for_task`) while keeping
/// everything in sync.
///
/// # Lock ordering
///
/// To prevent deadlocks, locks must **never** be held simultaneously across
/// two different fields.  All handlers follow the pattern of releasing one
/// guard before acquiring another (using scoped blocks `{ let g = …; … }`).
///
/// If a future change requires holding two locks at the same time, the
/// required acquisition order is:
///
/// ```text
/// documents → tinymist → executors → formatter → (path overrides)
/// ```
///
/// Always acquire locks in this order and release them in reverse order.
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
    pub preview_info: Arc<RwLock<Option<(String, u16)>>>,

    /// Monotonically increasing counter bumped on every `did_save`.
    ///
    /// Streaming compilation tasks compare their captured `gen` against this
    /// value to detect superseded saves and skip stale preview writes.
    pub compile_generation: Arc<AtomicU64>,

    /// Version counter for `textDocument/didChange` sent to tinymist for the
    /// preview `.typ` file.  Starts at 1 (matching `textDocument/didOpen`).
    /// Incremented on every write so each notification carries a strictly
    /// increasing version — a requirement of the LSP specification.
    pub preview_typ_version: Arc<AtomicU64>,

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
            preview_info: Arc::new(RwLock::new(None)),
            compile_generation: Arc::new(AtomicU64::new(0)),
            preview_typ_version: Arc::new(AtomicU64::new(1)),
            air_path_override: Arc::new(RwLock::new(None)),
            ruff_path_override: Arc::new(RwLock::new(None)),
            tinymist_path_override: Arc::new(RwLock::new(None)),
            loaded_snapshot_hash: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
