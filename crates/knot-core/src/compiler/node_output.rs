//! Per-node Typst output formatting for the execution pipeline.
//!
//! Each function corresponds to one rendering case: fresh execution,
//! inert (upstream error), skipped (`eval = false`), or execution error.
#![allow(missing_docs)]

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

/// Format a node with empty output in the given execution state.
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
        PlannedNodeKind::Chunk { node, .. } => {
            ("chunk", node.label.as_deref().unwrap_or("unnamed"))
        }
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

/// Delegates to the backend formatter, passing merged codly options separately.
pub(super) fn format_output(
    backend: &TypstBackend,
    chunk: &Chunk,
    merged_codly_options: &HashMap<String, String>,
    resolved_options: &ResolvedChunkOptions,
    output: &ExecutionOutput,
    state: &ChunkExecutionState,
) -> String {
    backend.format_chunk(chunk, merged_codly_options, resolved_options, output, state)
}
