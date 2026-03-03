//! Per-node Typst output formatting for the execution pipeline.
//!
//! Each function corresponds to one rendering case: fresh execution,
//! inert (upstream error), skipped (`eval = false`), or execution error.

use crate::backend::{Backend, TypstBackend};
use crate::compiler::pipeline::{ChunkExecutionState, PlannedNode, PlannedNodeKind};
use crate::config::Config;
use crate::executors::{ExecutionOutput, ExecutionResult};
use crate::parser::ResolvedChunkOptions;
use crate::parser::ast::Chunk;
use std::collections::HashMap;

/// Format the output of a freshly executed node.
pub(super) fn format_executed_node(
    pn: &PlannedNode,
    output: &ExecutionOutput,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    match &pn.kind {
        PlannedNodeKind::Chunk { node: chunk, data } => format_output(
            backend,
            chunk,
            &data.merged_codly_options,
            &data.resolved_options,
            output,
            state,
        ),
        PlannedNodeKind::Inline { .. } => match &output.result {
            ExecutionResult::Text(s) => s.clone(),
            _ => String::new(),
        },
    }
}

/// Format a node that is in the Inert state (upstream error, no execution).
pub(super) fn inert_output(pn: &PlannedNode, backend: &TypstBackend, config: &Config) -> String {
    match &pn.kind {
        PlannedNodeKind::Chunk { node: chunk, .. } => {
            let (_, inert_resolved, inert_codly) =
                super::options::resolve_options(chunk, config, &ChunkExecutionState::Inert);
            let empty = ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
            };
            format_output(
                backend,
                chunk,
                &inert_codly,
                &inert_resolved,
                &empty,
                &ChunkExecutionState::Inert,
            )
        }
        PlannedNodeKind::Inline { node: inline } => {
            format!(
                "#text(fill: luma(150))[`{{{} {}}}`]",
                inline.language, inline.code
            )
        }
    }
}

/// Format a node that is pending execution (placeholder: code shown, empty output).
///
/// Used during progressive compilation (Phase 0): the node is known to need
/// execution but hasn't run yet. Rendered identically to a skipped node —
/// code visible according to the chunk's `show` option, output empty.
pub(super) fn pending_output(pn: &PlannedNode, backend: &TypstBackend) -> String {
    match &pn.kind {
        PlannedNodeKind::Chunk { node: chunk, data } => {
            let empty = ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
            };
            format_output(
                backend,
                chunk,
                &data.merged_codly_options,
                &data.resolved_options,
                &empty,
                &ChunkExecutionState::Pending,
            )
        }
        PlannedNodeKind::Inline { .. } => String::new(),
    }
}

/// Format a node with eval = false (no execution, empty result).
pub(super) fn skip_output(
    pn: &PlannedNode,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    match &pn.kind {
        PlannedNodeKind::Chunk { node: chunk, data } => {
            let empty = ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
            };
            format_output(
                backend,
                chunk,
                &data.merged_codly_options,
                &data.resolved_options,
                &empty,
                state,
            )
        }
        PlannedNodeKind::Inline { .. } => String::new(),
    }
}

/// Format the Typst error block shown when a node fails to execute.
pub(super) fn format_error_block_for_node(
    kind: &PlannedNodeKind,
    lang: &str,
    error_msg: &str,
) -> String {
    let error_msg = error_msg.replace('"', "\\\"");
    let (node_kind, node_name) = match kind {
        PlannedNodeKind::Chunk { node, .. } => ("chunk", node.name.as_deref().unwrap_or("unnamed")),
        PlannedNodeKind::Inline { .. } => ("inline expression", "inline"),
    };
    format!(
        "#code-chunk(
    lang: \"{lang}\",
    is-inert: false,
    errors: ([#local(zebra-fill: none)[\n=== Execution Error ({lang})\nIn {node_kind} `{node_name}`\n\n```\n{error_msg}\n```\n\n_Execution of subsequent `{lang}` blocks has been suspended._]],)
)\n"
    )
}

/// Clones the chunk with merged codly options and delegates to the backend formatter.
pub(super) fn format_output(
    backend: &TypstBackend,
    chunk: &Chunk,
    merged_codly_options: &HashMap<String, String>,
    resolved_options: &ResolvedChunkOptions,
    output: &ExecutionOutput,
    state: &ChunkExecutionState,
) -> String {
    let mut chunk_with_codly = chunk.clone();
    chunk_with_codly.codly_options = merged_codly_options.clone();
    backend.format_chunk(&chunk_with_codly, resolved_options, output, state)
}
