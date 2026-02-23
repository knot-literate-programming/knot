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

use crate::compiler::ExecutableNode;
use crate::executors::ExecutionOutput;
use crate::parser::{ChunkOptions, ResolvedChunkOptions};
use std::collections::HashMap;

/// Execution lifecycle state of a chunk within the compilation pipeline.
///
/// - `Ready`   : cache valid or execution just succeeded — full output available.
/// - `Inert`   : follows an upstream error; rendered as raw code without execution.
/// - `Pending` : cache invalidated, not yet executed (reserved for progressive
///   compilation: first-pass placeholder before execution completes).
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkExecutionState {
    Ready,
    Inert,
    Pending,
}

/// What the execution phase must do with this node.
pub enum ExecutionNeed {
    /// Cache hit for a code chunk — `ExecutionOutput` already retrieved.
    CacheHit(ExecutionOutput),
    /// Cache hit for an inline expression — text result already retrieved.
    CacheHitInline(String),
    /// Cache miss — the node must be executed (slow path).
    MustExecute,
    /// Execution not requested (`eval = false`).
    Skip,
}

/// Chunk-specific data resolved during the planning phase.
pub struct ChunkPlanData {
    pub resolved_options: ResolvedChunkOptions,
    pub chunk_options: ChunkOptions,
    pub merged_codly_options: HashMap<String, String>,
    pub name: String,
}

/// A node after the planning phase: hash computed, cache checked, no code executed yet.
pub struct PlannedNode {
    pub node: ExecutableNode,
    pub lang: String,
    pub hash: String,
    pub previous_hash: String,
    pub source_start: usize,
    pub source_end: usize,
    /// Present for chunk nodes; `None` for inline expressions.
    pub chunk_data: Option<ChunkPlanData>,
    /// What the execution phase must do with this node.
    pub need: ExecutionNeed,
}

/// A node after the execution phase: Typst output is fully determined.
///
/// Owns all its data — no lifetime dependency on the source document.
pub struct ExecutedNode {
    pub lang: String,
    pub hash: String,
    pub source_start: usize,
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
