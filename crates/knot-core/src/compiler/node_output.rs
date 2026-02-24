//! Per-node Typst output formatting for the execution pipeline.
//!
//! Each function corresponds to one rendering case: fresh execution,
//! inert (upstream error), skipped (`eval = false`), or execution error.

use crate::backend::{Backend, TypstBackend};
use crate::compiler::pipeline::ChunkExecutionState;
use crate::compiler::pipeline::PlannedNode;
use crate::config::Config;
use crate::executors::{ExecutionOutput, ExecutionResult};
use crate::parser::ResolvedChunkOptions;
use crate::parser::ast::Chunk;
use std::collections::HashMap;

use super::ExecutableNode;

/// Format the output of a freshly executed node.
pub(super) fn format_executed_node(
    pn: &PlannedNode,
    output: &ExecutionOutput,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
            let data = pn.chunk_data.as_ref().unwrap();
            format_output(
                backend,
                chunk,
                &data.merged_codly_options,
                &data.resolved_options,
                output,
                state,
            )
        }
        ExecutableNode::InlineExpr(_) => match &output.result {
            ExecutionResult::Text(s) => s.clone(),
            _ => String::new(),
        },
    }
}

/// Format a node that is in the Inert state (upstream error, no execution).
pub(super) fn inert_output(pn: &PlannedNode, backend: &TypstBackend, config: &Config) -> String {
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
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
        ExecutableNode::InlineExpr(inline) => {
            format!(
                "#text(fill: luma(150))[`{{{} {}}}`]",
                inline.language, inline.code
            )
        }
    }
}

/// Format a node with eval = false (no execution, empty result).
pub(super) fn skip_output(
    pn: &PlannedNode,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
            let data = pn.chunk_data.as_ref().unwrap();
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
        ExecutableNode::InlineExpr(_) => String::new(),
    }
}

/// Format the Typst error block shown when a node fails to execute.
pub(super) fn format_error_block_for_node(
    node: &ExecutableNode,
    lang: &str,
    error_msg: &str,
) -> String {
    let error_msg = error_msg.replace('"', "\\\"");
    let node_kind = match node {
        ExecutableNode::Chunk(_) => "chunk",
        ExecutableNode::InlineExpr(_) => "inline expression",
    };
    let node_name = match node {
        ExecutableNode::Chunk(c) => c.name.as_deref().unwrap_or("unnamed"),
        ExecutableNode::InlineExpr(_) => "inline",
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
