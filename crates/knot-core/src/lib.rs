//! `knot-core` — parsing, compilation, caching and execution engine for Knot.
//!
//! # Architecture
//!
//! Compilation proceeds in three passes driven by [`compiler::Compiler`]:
//!
//! 1. **Plan** — [`compiler::pipeline`] resolves options, computes SHA-256 hashes
//!    and classifies each node as `Skip`, `CacheHit` or `MustExecute`.
//! 2. **Execute** — `MustExecute` nodes are grouped by language and run in parallel
//!    threads; results are written to the on-disk [`cache`].
//! 3. **Assemble** — node outputs are interleaved with the raw source to produce a
//!    `.typ` file for Typst.
//!
//! For progressive preview, [`project::compile_project_phase0`] runs only Pass 1,
//! rendering cached chunks immediately and placeholders for pending ones.
//! [`project::compile_project_full`] runs all three passes with optional streaming.

pub mod backend;
pub mod cache;
pub mod cancellation;
mod cleaning;
pub mod compiler;
pub mod config;
pub mod defaults;
pub mod executors;
pub mod formatting;
pub mod graphics;
pub mod parser;
pub mod path_utils;
pub mod project;
pub mod tools;
mod typst_syntax;

pub use backend::{format_codly_call, format_local_call};
pub use cleaning::{CleanSummary, clean_project, clean_project_with_summary};
pub use compiler::Compiler;
pub use compiler::formatters::CodeFormatter;
pub use compiler::sync;
pub use compiler::{
    BuildDiagnostic, CompiledDocument, ExecutedNode, Phase0Mode, PlannedNode, ProgressEvent,
    assemble_pass, planned_to_partial_nodes,
};
pub use config::{ChunkDefaults, Config};
pub use defaults::Defaults;
pub use graphics::{GraphicsDefaults, ResolvedGraphicsOptions, resolve_graphics_options};
pub use parser::{Chunk, ChunkOptions, Document, InlineExpr, ResolvedChunkOptions};
pub use project::{
    ProjectOutput, ProjectPaths, compile_project_full, compile_project_phase0,
    compile_project_phase0_unsaved, fix_paths_in_typst,
};

/// Typst library embedded in the binary, prepended to every assembled `.typ` file.
///
/// Provides `code-chunk` and `knot-state-styles`. Inlined by the assembler so
/// projects do not need a `lib/` directory or an `#import` statement.
pub const LIB_TYP: &str = include_str!("../../../knot-typst-package/lib.typ");

/// R helper scripts embedded in the binary, loaded into every R executor session.
///
/// Each entry is `(filename, source_code)`. Scripts are sourced in order.
pub const R_HELPERS: &[(&str, &str)] = &[
    ("helpers.R", include_str!("../resources/r/helpers.R")),
    ("executor.R", include_str!("../resources/r/executor.R")),
    ("session.R", include_str!("../resources/r/session.R")),
    ("output.R", include_str!("../resources/r/output.R")),
    ("lsp.R", include_str!("../resources/r/lsp.R")),
];

/// Python helper scripts embedded in the binary, loaded into every Python executor session.
///
/// Each entry is `(filename, source_code)`. Scripts are executed in order.
pub const PYTHON_HELPERS: &[(&str, &str)] = &[
    ("helpers.py", include_str!("../resources/python/helpers.py")),
    (
        "executor.py",
        include_str!("../resources/python/executor.py"),
    ),
    ("session.py", include_str!("../resources/python/session.py")),
    ("output.py", include_str!("../resources/python/output.py")),
    ("lsp.py", include_str!("../resources/python/lsp.py")),
];

/// A short fingerprint of the embedded language scripts (R + Python).
///
/// Included in every chunk hash so that updating `output.R` or `output.py`
/// automatically invalidates all existing cache entries — no manual
/// `knot clean` required when the runtime scripts change.
///
/// Computed once at first access and cached for the lifetime of the process.
pub static SCRIPTS_VERSION: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (_, content) in R_HELPERS {
        hasher.update(content.as_bytes());
    }
    for (_, content) in PYTHON_HELPERS {
        hasher.update(content.as_bytes());
    }
    format!("{:x}", hasher.finalize())[..16].to_string()
});

use std::path::{Path, PathBuf};

/// Return a versioned cache directory keyed by the complete document path.
/// Existing paths are canonicalized so symlink/relative aliases share a cache,
/// while equal basenames in different directories never share execution state.
pub fn get_cache_dir(project_root: &Path, document_path: impl AsRef<Path>) -> PathBuf {
    use sha2::{Digest, Sha256};
    let path = document_path.as_ref();
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let identity = path.canonicalize().unwrap_or(path);
    let key = format!(
        "{:x}",
        Sha256::digest(identity.as_os_str().as_encoded_bytes())
    );
    project_root
        .join(Defaults::CACHE_DIR_NAME)
        .join("v2")
        .join(key)
}
