//! Pass 2 — parallel execution of language chains.
//!
//! Nodes of the same language run sequentially (shared interpreter state);
//! nodes of different languages run in separate OS threads simultaneously.

use crate::backend::TypstBackend;
use crate::cache::Cache;
use crate::compiler::pipeline::{
    ChunkExecutionState, ExecutedNode, ExecutionNeed, PlannedNode, PlannedNodeKind,
};
use crate::compiler::snapshot_manager::SnapshotManager;
use crate::config::Config;
use crate::executors::{
    ExecutionAttempt, ExecutionOutput, ExecutionResult, GraphicsOptions, KnotExecutor,
};
use crate::parser::Show;
use anyhow::{Context, Result};
use log::info;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Emitted after each node completes execution, enabling progressive preview updates.
///
/// `doc_idx` is the node's global position in the original planned document order
/// (same index used by `assemble_pass`). Use it to update a `Vec<ExecutedNode>`
/// buffer in place, then reassemble the Typst output.
pub struct ProgressEvent {
    /// Global index of the node in the original planned document order.
    ///
    /// Matches the index used by [`assemble_pass`](crate::assemble_pass).
    /// Use it to update a `Vec<ExecutedNode>` buffer in place.
    pub doc_idx: usize,
    /// The fully executed node, ready to be inserted into the output buffer.
    pub executed: ExecutedNode,
}

use super::freeze::{check_freeze_contract, register_freeze_objects};
use super::node_output::{
    format_error_block_for_node, format_executed_node, format_output, inert_output, skip_output,
};

/// Return type of [`run_language_chain`]: language tag, executor, and indexed nodes.
pub(super) type ChainOutput = (
    String,
    Option<Box<dyn KnotExecutor>>,
    Vec<(usize, ExecutedNode)>,
);

/// Immutable per-chain context threaded through execution helpers.
struct ChainContext<'a> {
    lang: &'a str,
    cache: &'a Arc<Mutex<Cache>>,
    backend: &'a TypstBackend,
    project_root: &'a Path,
}

/// Group planned nodes by language, preserving their original document indices.
pub(super) fn group_by_language(
    planned: Vec<PlannedNode>,
) -> HashMap<String, Vec<(usize, PlannedNode)>> {
    let mut groups: HashMap<String, Vec<(usize, PlannedNode)>> = HashMap::new();
    for (i, pn) in planned.into_iter().enumerate() {
        groups.entry(pn.lang.clone()).or_default().push((i, pn));
    }
    groups
}

/// Execute all nodes for a single language sequentially, returning results tagged
/// with their original document indices (for later reassembly in document order).
///
/// Returns `(lang, executor, indexed_nodes)` — the executor is returned so the
/// caller can put it back into the `ExecutorManager`.
///
/// When `progress` is `Some`, a [`ProgressEvent`] is sent after each node
/// completes. `Sender::send` is non-blocking and safe to call from any thread.
// All arguments are required: lang+nodes (chain identity), exec (executor ownership),
// cache (shared state), backend+config+project_root (render context), progress (streaming).
// Grouping them into a struct would not reduce coupling and would scatter the call sites.
#[expect(clippy::too_many_arguments)]
pub(super) fn run_language_chain(
    lang: String,
    nodes: Vec<(usize, PlannedNode)>,
    exec: Option<Box<dyn KnotExecutor>>,
    cache: Arc<Mutex<Cache>>,
    backend: &TypstBackend,
    config: &Config,
    project_root: &Path,
    progress: Option<Sender<ProgressEvent>>,
) -> Result<ChainOutput> {
    let ctx = ChainContext {
        lang: &lang,
        cache: &cache,
        backend,
        project_root,
    };
    let mut sm = SnapshotManager::new(exec);
    let mut indexed = Vec::with_capacity(nodes.len());
    let mut broken = false;

    for (doc_idx, pn) in nodes {
        let (is_chunk, source_line) = match &pn.kind {
            PlannedNodeKind::Chunk { node, .. } => (true, (node.range.start.line + 1) as u32),
            PlannedNodeKind::Inline { .. } => (false, 0),
        };

        let state = if broken {
            ChunkExecutionState::Inert
        } else {
            ChunkExecutionState::Ready
        };

        let (typst_content, errored) = process_node(&pn, &state, &mut sm, &ctx, config)?;
        if errored {
            broken = true;
        }

        let executed = ExecutedNode {
            lang: lang.clone(),
            hash: pn.hash,
            source_start: pn.source_start,
            source_end: pn.source_end,
            typst_content,
            is_chunk,
            source_line,
            errored,
        };

        if let Some(tx) = &progress {
            let _ = tx.send(ProgressEvent {
                doc_idx,
                executed: executed.clone(),
            });
        }

        indexed.push((doc_idx, executed));
    }

    Ok((lang, sm.into_executor(), indexed))
}

/// Dispatch a single planned node to the appropriate execution path.
///
/// Returns `(typst_content, errored)`. Any `Err` propagated here is an
/// infrastructure failure (snapshot I/O), not a language-level error.
fn process_node(
    pn: &PlannedNode,
    state: &ChunkExecutionState,
    sm: &mut SnapshotManager,
    ctx: &ChainContext<'_>,
    config: &Config,
) -> Result<(String, bool)> {
    if matches!(state, ChunkExecutionState::Inert) {
        return Ok((inert_output(pn, ctx.backend, config), false));
    }

    match &pn.need {
        ExecutionNeed::CacheHit(attempt) => {
            let (chunk, data) = match &pn.kind {
                PlannedNodeKind::Chunk { node, data } => (node, data),
                PlannedNodeKind::Inline { .. } => unreachable!("CacheHit only for chunks"),
            };
            info!("  ✓ {} [cached]", data.name);
            match attempt {
                ExecutionAttempt::RuntimeError(error) => Ok((
                    format_error_block_for_node(&pn.kind, ctx.lang, &error.to_string()),
                    true,
                )),
                ExecutionAttempt::Success(output) => {
                    let content = format_output(
                        ctx.backend,
                        chunk,
                        &data.merged_codly_options,
                        &data.resolved_options,
                        output,
                        state,
                    );
                    {
                        let cache_guard = ctx.cache.lock().unwrap();
                        sm.update_after_node(
                            ctx.lang,
                            &pn.hash,
                            &pn.previous_hash,
                            &cache_guard,
                            ctx.project_root,
                        )?;
                    }
                    Ok((content, false))
                }
            }
        }

        ExecutionNeed::CacheHitInline(result) => {
            info!("  ✓ [cached inline]");
            let result_clone = result.clone();
            {
                let cache_guard = ctx.cache.lock().unwrap();
                sm.update_after_node(
                    ctx.lang,
                    &pn.hash,
                    &pn.previous_hash,
                    &cache_guard,
                    ctx.project_root,
                )?;
            }
            Ok((result_clone, false))
        }

        ExecutionNeed::Skip => {
            let content = skip_output(pn, ctx.backend, state);
            {
                let cache_guard = ctx.cache.lock().unwrap();
                sm.update_after_node(
                    ctx.lang,
                    &pn.hash,
                    &pn.previous_hash,
                    &cache_guard,
                    ctx.project_root,
                )?;
            }
            Ok((content, false))
        }

        ExecutionNeed::MustExecute => handle_must_execute(pn, sm, ctx),
    }
}

/// Execute a node that has no valid cache entry.
///
/// Handles snapshot restoration, execution, freeze contract checks, and result
/// caching. Returns `(typst_content, errored)`.
fn handle_must_execute(
    pn: &PlannedNode,
    sm: &mut SnapshotManager,
    ctx: &ChainContext<'_>,
) -> Result<(String, bool)> {
    // Restore session snapshot before executing.
    // Lock only for the read, release before executing.
    {
        let cache_guard = ctx.cache.lock().unwrap();
        sm.restore_if_needed(ctx.lang, &pn.previous_hash, &cache_guard, ctx.project_root)?;
    }

    // All executor interactions are confined to this block so that the borrow
    // of sm.exec ends before sm.update_after_node is called below.
    let output = {
        // Language not supported: show an error block and cascade Inert.
        // Not cached — an unsupported language is not a deterministic runtime state.
        let exec = match sm.executor_mut() {
            None => {
                return Ok((
                    format_error_block_for_node(
                        &pn.kind,
                        ctx.lang,
                        &format!("Unsupported language: '{}'", ctx.lang),
                    ),
                    true,
                ));
            }
            Some(e) => e,
        };

        // Infrastructure failure (process crash, timeout…): error block, cascade Inert.
        // Not cached — not a deterministic runtime state.
        let attempt = match execute_for_node(pn, exec, ctx.cache) {
            Err(e) => {
                return Ok((
                    format_error_block_for_node(&pn.kind, ctx.lang, &e.to_string()),
                    true,
                ));
            }
            Ok(a) => a,
        };

        // Runtime error → cache it, then cascade Inert.
        let output = match attempt {
            ExecutionAttempt::RuntimeError(error) => {
                cache_chunk_error(pn, &error, ctx.cache)?;
                return Ok((
                    format_error_block_for_node(&pn.kind, ctx.lang, &error.to_string()),
                    true,
                ));
            }
            ExecutionAttempt::Success(output) => output,
        };

        // Successful execution: register freeze objects if declared.
        if let PlannedNodeKind::Chunk { node: chunk, .. } = &pn.kind
            && !chunk.options.freeze.is_empty()
        {
            register_freeze_objects(chunk, exec, ctx.cache, ctx.project_root)?;
        }

        // Check freeze contract.
        // IMPORTANT: save_result is only called when the contract passes.
        // A violating chunk must NOT be cached as success — if it were,
        // the check would be bypassed (CacheHit path) on every subsequent run.
        if let Some(violation) = check_freeze_contract(pn, exec, ctx.cache)? {
            // Contract violated: cache as error so LSP shows full details, then cascade.
            cache_chunk_error(pn, &violation, ctx.cache)?;
            return Ok((
                format_error_block_for_node(&pn.kind, ctx.lang, &violation.to_string()),
                true,
            ));
        }

        output
    }; // exec (and its borrow of sm.exec) is released here.

    // Contract OK: persist result to cache, advance snapshot pointer.
    cache_chunk_result(pn, &output, ctx.cache)?;
    {
        let cache_guard = ctx.cache.lock().unwrap();
        sm.update_after_node(
            ctx.lang,
            &pn.hash,
            &pn.previous_hash,
            &cache_guard,
            ctx.project_root,
        )?;
    }
    Ok((
        format_executed_node(pn, &output, ctx.backend, &ChunkExecutionState::Ready),
        false,
    ))
}

/// Persist a runtime error to the cache (if caching is enabled for this node).
fn cache_chunk_error(
    pn: &PlannedNode,
    error: &crate::executors::RuntimeError,
    cache: &Arc<Mutex<Cache>>,
) -> Result<()> {
    if let PlannedNodeKind::Chunk { node: chunk, data } = &pn.kind
        && data.resolved_options.cache
    {
        cache.lock().unwrap().save_error(
            chunk.index,
            chunk.name.clone(),
            chunk.language.clone(),
            pn.hash.clone(),
            error.clone(),
            data.chunk_options.depends.clone(),
        )?;
    }
    Ok(())
}

/// Persist a successful execution result to the cache (if caching is enabled).
fn cache_chunk_result(
    pn: &PlannedNode,
    output: &ExecutionOutput,
    cache: &Arc<Mutex<Cache>>,
) -> Result<()> {
    if let PlannedNodeKind::Chunk { node: chunk, data } = &pn.kind
        && data.resolved_options.cache
    {
        cache.lock().unwrap().save_result(
            chunk.index,
            chunk.name.clone(),
            chunk.language.clone(),
            pn.hash.clone(),
            output,
            data.chunk_options.depends.clone(),
        )?;
    }
    Ok(())
}

/// Execute the node's code and return the raw output.
///
/// Snapshot restoration is the caller's responsibility and must happen before
/// this call.  Only called for `ExecutionNeed::MustExecute` nodes.
///
/// **Does not persist to cache.** Caching is done by the caller so that chunk
/// results are only saved after all post-execution checks (e.g. freeze contract)
/// have passed.  Saving before those checks would mark a violating chunk as a
/// cache hit, silently bypassing the check on every subsequent run.
fn execute_for_node(
    pn: &PlannedNode,
    exec: &mut Box<dyn KnotExecutor>,
    cache: &Arc<Mutex<Cache>>,
) -> Result<ExecutionAttempt> {
    match &pn.kind {
        PlannedNodeKind::Chunk { node: chunk, data } => {
            info!("  ⚙️ {} [executing]", data.name);

            let graphics_opts = GraphicsOptions {
                width: data.resolved_options.fig_width,
                height: data.resolved_options.fig_height,
                dpi: data.resolved_options.dpi,
                format: data.resolved_options.fig_format.as_str().to_string(),
            };

            exec.execute(&chunk.code, &graphics_opts)
        }

        PlannedNodeKind::Inline { node: inline } => {
            info!("  ⚙️ `{{{}}} {}` [executing]", inline.language, inline.code);

            let result = exec.execute_inline(&inline.code).context(format!(
                "Failed to execute inline expression: `{{{}}} {}`",
                inline.language, inline.code
            ))?;

            let resolved = inline.options.resolve();
            let final_result = match resolved.show {
                Show::Output | Show::Both => result,
                Show::Code | Show::None => String::new(),
            };

            cache
                .lock()
                .unwrap()
                .save_inline_result(pn.hash.clone(), &final_result)?;

            Ok(ExecutionAttempt::Success(ExecutionOutput {
                result: ExecutionResult::Text(final_result),
                warnings: vec![],
            }))
        }
    }
}
