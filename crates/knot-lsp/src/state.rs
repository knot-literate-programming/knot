use crate::position_mapper::PositionMapper;
use crate::proxy::TinymistProxy;
use knot_core::CodeFormatter;
use knot_core::executors::ExecutorManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, RwLock};
use tower_lsp::lsp_types::{Diagnostic, Url};

/// In-memory overlay state for `main.typ` in the Tinymist subprocess.
///
/// The overlay allows Tinymist to update the browser preview instantly (without
/// waiting for macOS FSEvents debouncing) by keeping the file content in memory.
///
/// Transition: `Inactive → Active` when `textDocument/didOpen` is sent.
/// The overlay becomes `Active` in `do_start_preview` after the first preview
/// starts; it stays `Active` for the lifetime of the LSP session.
pub enum TinymistOverlay {
    /// `textDocument/didOpen` not yet sent.
    /// Tinymist monitors the disk file (normal mode).
    Inactive,

    /// `textDocument/didOpen` sent with version = 1.
    /// Tinymist holds the file in memory.
    /// `next_version`: version number for the next `textDocument/didChange`.
    /// Starts at 2 (1 is reserved for `didOpen`).
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
/// documents → tinymist_overlay → tinymist → executors → formatter → (path overrides)
/// ```
///
/// Always acquire locks in this order and release them in reverse order.
/// In practice: acquire `tinymist_overlay` (write, get/increment version, release),
/// **then** acquire `tinymist` (read, get proxy, release). Never hold both simultaneously.
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

    /// In-memory overlay state for `main.typ` in Tinymist.
    /// See [`TinymistOverlay`] for the state machine.
    pub tinymist_overlay: Arc<RwLock<TinymistOverlay>>,

    /// Monotonic counter incremented on every `did_save`.
    /// Background compile tasks skip stale preview writes when this has changed.
    pub compile_generation: Arc<AtomicU64>,

    /// Per-document debounce handles for the Phase-0-only compile triggered by
    /// `did_change`.  The previous handle is aborted on each new keystroke so
    /// only the last one (after the typing pause) actually fires.
    pub debounce_handles: Arc<Mutex<HashMap<Url, tokio::task::JoinHandle<()>>>>,

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
            tinymist_overlay: Arc::new(RwLock::new(TinymistOverlay::Inactive)),
            compile_generation: Arc::new(AtomicU64::new(0)),
            debounce_handles: Arc::new(Mutex::new(HashMap::new())),
            air_path_override: Arc::new(RwLock::new(None)),
            ruff_path_override: Arc::new(RwLock::new(None)),
            tinymist_path_override: Arc::new(RwLock::new(None)),
            loaded_snapshot_hash: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
