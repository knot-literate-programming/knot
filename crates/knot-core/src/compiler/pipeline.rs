//! Pipeline stage types for the three-pass compilation model.
//!
//! The compilation pipeline is structured as three explicit phases:
//!
//! 1. **Plan**     — resolve options, compute hashes, check cache.
//!    Produces: [`PlannedNode`]
//!
//! 2. **Execute**  — run pending nodes, handle errors, format output.
//!    Produces: [`ExecutedNode`]
//!
//! 3. **Assemble** — interleave node outputs with the raw source text.
//!    Produces: the final Typst `String`
//!
//! This separation is the foundation for progressive compilation: after the
//! planning phase we know *exactly* which nodes are already cached
//! ([`ExecutionNeed::CacheHit`] / [`ExecutionNeed::CacheHitInline`]) and
//! which still need execution ([`ExecutionNeed::MustExecute`]).  The cached
//! nodes can be rendered immediately while the pending ones run in the
//! background.

use crate::executors::ExecutionAttempt;
use crate::parser::ast::{Chunk, InlineExpr};
use crate::parser::{ChunkOptions, ResolvedChunkOptions};
use std::collections::HashMap;

/// Execution lifecycle state of a chunk within the compilation pipeline.
///
/// - `Ready`           : cache valid or execution just succeeded — full output available.
/// - `Inert`           : follows an upstream error; rendered as raw code without execution.
/// - `Pending`         : compilation in progress (`do_compile` Phase 0) — orange border.
/// - `Modified`        : first `MustExecute` in a language chain; user directly edited
///   this chunk — amber border (strong).
/// - `ModifiedCascade` : subsequent `MustExecute` in the same chain; hash-invalidated
///   downstream chunk — amber border (muted).
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkExecutionState {
    /// Cache valid or execution succeeded — full output available.
    Ready,
    /// Follows an upstream error; rendered as raw code without execution.
    Inert,
    /// Compilation in progress (`do_compile` Phase 0) — orange border.
    Pending,
    /// First `MustExecute` in a language chain; user directly edited this chunk — amber border (strong).
    Modified,
    /// Subsequent `MustExecute` in the same chain; hash-invalidated downstream chunk — amber border (muted).
    ModifiedCascade,
}

/// What the execution phase must do with this node.
pub enum ExecutionNeed {
    /// Cache hit for a code chunk — `ExecutionAttempt` already retrieved.
    CacheHit(ExecutionAttempt),
    /// Cache hit for an inline expression — text result already retrieved.
    CacheHitInline(String),
    /// Cache miss — the node must be executed (slow path).
    MustExecute,
    /// Execution not requested (`eval = false`).
    Skip,
}

/// Chunk-specific data resolved during the planning phase.
pub struct ChunkPlanData {
    /// Fully resolved chunk options (merged from config + per-chunk overrides).
    pub resolved_options: ResolvedChunkOptions,
    /// Raw per-chunk options (before merging), kept for hash computation.
    pub chunk_options: ChunkOptions,
    /// Codly presentation options merged from global config and chunk-level overrides.
    pub merged_codly_options: HashMap<String, String>,
    /// Chunk label (name or auto-generated).
    pub name: String,
}

/// The node-type-specific part of a planned node.
///
/// The compiler guarantees that `ChunkPlanData` is always present for chunk
/// nodes and absent for inline expressions — this invariant is encoded in the
/// type rather than relying on `Option::unwrap`.
pub enum PlannedNodeKind {
    /// A fenced code chunk.
    Chunk {
        /// The parsed chunk AST node.
        node: Box<Chunk>,
        /// Planning data resolved for this chunk.
        data: Box<ChunkPlanData>,
    },
    /// An inline expression.
    Inline {
        /// The parsed inline expression AST node.
        node: InlineExpr,
    },
}

/// A node after the planning phase: hash computed, cache checked, no code executed yet.
pub struct PlannedNode {
    /// Type-specific data (chunk or inline expression).
    pub kind: PlannedNodeKind,
    /// Language identifier (e.g. `"r"`, `"python"`).
    pub lang: String,
    /// SHA-256 hash of this node (chained with the previous node's hash).
    pub hash: String,
    /// Hash of the preceding node in the same language chain (used for chain invalidation).
    pub previous_hash: String,
    /// Byte offset in the source document where this node begins.
    pub source_start: usize,
    /// Byte offset in the source document where this node ends.
    pub source_end: usize,
    /// What the execution phase must do with this node.
    pub need: ExecutionNeed,
}

/// A node after the execution phase: Typst output is fully determined.
///
/// Owns all its data — no lifetime dependency on the source document.
#[derive(Clone)]
pub struct ExecutedNode {
    /// Language identifier (e.g. `"r"`, `"python"`).
    pub lang: String,
    /// SHA-256 hash of this node (used to update the cache entry).
    pub hash: String,
    /// Byte offset in the source document where this node begins.
    pub source_start: usize,
    /// Byte offset in the source document where this node ends.
    pub source_end: usize,
    /// Formatted Typst content ready to be inserted into the final output.
    pub typst_content: String,
    /// `true` for code chunks; `false` for inline expressions.
    pub is_chunk: bool,
    /// 1-based source line (chunk nodes only — for `#KNOT-SYNC` markers).
    pub source_line: u32,
    /// `true` if this node caused an execution error (triggers Inert cascade).
    pub errored: bool,
}
