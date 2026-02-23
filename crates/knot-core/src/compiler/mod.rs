use crate::cache::FreezeObjectInfo;
use crate::config::Config;
use crate::executors::side_channel::RuntimeError;
use crate::executors::{
    ExecutionOutput, ExecutionResult, ExecutorManager, GraphicsOptions, KnotExecutor,
};
use crate::parser::ast::{Chunk, Document, InlineExpr};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod chunk_processor;
pub mod formatters;
pub mod inline_processor;
pub mod pipeline;
pub mod snapshot_manager;
pub mod sync;

pub use chunk_processor::{ChunkExecutionState, ChunkProcessor};
pub use pipeline::{ExecutedNode, ExecutionNeed, PlannedNode};

/// Represents a node in the document that can be executed.
pub enum ExecutableNode {
    Chunk(Chunk),
    InlineExpr(InlineExpr),
}

use crate::backend::TypstBackend;
use crate::cache::Cache;
use crate::compiler::pipeline::ChunkPlanData;
use crate::compiler::snapshot_manager::SnapshotManager;
use crate::defaults::Defaults;
use crate::get_cache_dir;
use anyhow::{Context, Result};
use log::info;
use std::path::PathBuf;

pub struct Compiler {
    executor_manager: ExecutorManager,
    config: Config,
    project_root: PathBuf,
    cache_dir: PathBuf,
}

impl Compiler {
    /// Create a new compiler, searching for knot.toml starting from the given file path.
    pub fn new(knot_file_path: &Path) -> Result<Self> {
        let project_root = Config::find_project_root(knot_file_path)?;

        let config_path = project_root.join("knot.toml");
        let config = if config_path.exists() {
            Config::load_from_path(&config_path)?
        } else {
            Config::default()
        };

        let file_stem = knot_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);

        info!("📦 Cache directory: {}", cache_dir.display());

        let executor_manager = ExecutorManager::with_timeout(
            cache_dir.clone(),
            Duration::from_secs(config.execution.timeout_secs),
        );

        Ok(Self {
            executor_manager,
            config,
            project_root,
            cache_dir,
        })
    }

    /// Reset all active executors to a clean state.
    pub fn reset_executors(&mut self) {
        self.executor_manager.shutdown_all();
    }

    /// Compiles a document by executing its code chunks and generating a Typst source string.
    ///
    /// `source_file` is the filename of the `.knot` source (e.g. `"chapter1.knot"`).
    pub fn compile(&mut self, doc: &Document, source_file: &str) -> Result<String> {
        let cache = Arc::new(Mutex::new(Cache::new(self.cache_dir.clone())?));
        let backend = TypstBackend::new();

        let nodes = build_executable_nodes(doc);
        info!("🔧 Processing {} executable nodes...", nodes.len());

        // Pass 1: resolve options, compute hashes, check cache — no code executed.
        let planned = self.plan_pass(nodes, &cache)?;

        // Pass 2: execute pending nodes in parallel per language, format output.
        let executed = self.execute_pass(planned, Arc::clone(&cache), &backend)?;

        // Pass 3: interleave node outputs with source text.
        let typst_output = assemble_pass(&executed, &doc.source, source_file);

        info!("✓ All nodes processed.");
        cache.lock().unwrap().save_metadata()?;

        Ok(typst_output)
    }

    // -----------------------------------------------------------------------
    // Pass 1 — Plan
    // -----------------------------------------------------------------------

    /// For every node: resolve options, compute hash, check cache.
    /// Returns a `PlannedNode` for each node — no code is executed.
    fn plan_pass(
        &mut self,
        nodes: Vec<ExecutableNode>,
        cache: &Arc<Mutex<Cache>>,
    ) -> Result<Vec<PlannedNode>> {
        use chunk_processor::{compute_hash, resolve_options};

        // Lock once for the entire planning pass (synchronous, no contention).
        let cache = cache.lock().unwrap();

        let mut planned = Vec::with_capacity(nodes.len());
        let mut last_hash_per_lang: HashMap<String, String> = HashMap::new();

        for node in nodes {
            let lang = match &node {
                ExecutableNode::Chunk(c) => c.language.clone(),
                ExecutableNode::InlineExpr(e) => e.language.clone(),
            };
            let previous_hash = last_hash_per_lang.get(&lang).cloned().unwrap_or_default();
            let (source_start, source_end) = match &node {
                ExecutableNode::Chunk(c) => (c.start_byte, c.end_byte),
                ExecutableNode::InlineExpr(e) => (e.start, e.end),
            };

            let (hash, need, chunk_data) = match &node {
                ExecutableNode::Chunk(chunk) => {
                    let (chunk_options, resolved_options, merged_codly_options) =
                        resolve_options(chunk, &self.config, &ChunkExecutionState::Ready);
                    let name = chunk
                        .name
                        .as_deref()
                        .map(String::from)
                        .unwrap_or_else(|| format!("chunk-{}", chunk.index));
                    let hash = compute_hash(&chunk.code, &chunk_options, &previous_hash)?;
                    let need = if !resolved_options.eval {
                        ExecutionNeed::Skip
                    } else if resolved_options.cache && cache.has_cached_result(&hash) {
                        ExecutionNeed::CacheHit(cache.get_cached_result(&hash)?)
                    } else {
                        ExecutionNeed::MustExecute
                    };
                    let data = ChunkPlanData {
                        resolved_options,
                        chunk_options,
                        merged_codly_options,
                        name,
                    };
                    (hash, need, Some(data))
                }
                ExecutableNode::InlineExpr(inline) => {
                    let resolved = inline.options.resolve();
                    let hash =
                        cache.get_inline_expr_hash(&inline.code, &inline.options, &previous_hash);
                    let need = if !resolved.eval {
                        ExecutionNeed::Skip
                    } else if cache.has_cached_inline_result(&hash) {
                        ExecutionNeed::CacheHitInline(cache.get_cached_inline_result(&hash)?)
                    } else {
                        ExecutionNeed::MustExecute
                    };
                    (hash, need, None)
                }
            };

            last_hash_per_lang.insert(lang.clone(), hash.clone());
            planned.push(PlannedNode {
                node,
                lang,
                hash,
                previous_hash,
                source_start,
                source_end,
                chunk_data,
                need,
            });
        }

        Ok(planned)
    }

    // -----------------------------------------------------------------------
    // Pass 2 — Execute (parallel per language)
    // -----------------------------------------------------------------------

    /// Execute pending nodes in parallel per language, format all outputs.
    ///
    /// Nodes of the same language run sequentially (shared interpreter state);
    /// nodes of different languages run in separate OS threads simultaneously.
    fn execute_pass(
        &mut self,
        planned: Vec<PlannedNode>,
        cache: Arc<Mutex<Cache>>,
        backend: &TypstBackend,
    ) -> Result<Vec<ExecutedNode>> {
        // Step 1: group nodes by language, preserving document order via indices.
        let groups = group_by_language(planned);

        // Step 2: for languages with MustExecute nodes, ensure the executor is
        // initialized (lazy), then take it for exclusive use in its thread.
        let mut chain_executors: HashMap<String, Box<dyn KnotExecutor>> = HashMap::new();
        for (lang, nodes) in &groups {
            let needs_exec = nodes
                .iter()
                .any(|(_, pn)| matches!(pn.need, ExecutionNeed::MustExecute));
            if needs_exec {
                // Initialize if needed; ignore failure — the chain will produce an error block.
                let _ = self.executor_manager.get_executor(lang);
            }
            if let Some(exec) = self.executor_manager.take(lang) {
                chain_executors.insert(lang.clone(), exec);
            }
        }

        // Clone immutable data once so threads can borrow it without capturing `self`.
        let config = self.config.clone();
        let project_root = self.project_root.clone();

        // Step 3: build per-chain inputs (each chain owns its executor).
        let chain_data: Vec<(String, Vec<(usize, PlannedNode)>, Option<Box<dyn KnotExecutor>>)> =
            groups
                .into_iter()
                .map(|(lang, nodes)| {
                    let exec = chain_executors.remove(&lang);
                    (lang, nodes, exec)
                })
                .collect();

        // Step 4: run each language chain in its own OS thread.
        type ChainResult =
            Result<(String, Option<Box<dyn KnotExecutor>>, Vec<(usize, ExecutedNode)>)>;

        // Reborrow as references so closures can copy them (references are Copy).
        let config_ref = &config;
        let project_root_ref = &project_root;

        let chain_results: Vec<ChainResult> = std::thread::scope(|s| {
            let handles: Vec<_> = chain_data
                .into_iter()
                .map(|(lang, nodes, exec)| {
                    let cache = Arc::clone(&cache);
                    s.spawn(move || {
                        run_language_chain(
                            lang,
                            nodes,
                            exec,
                            cache,
                            SnapshotManager::new(),
                            backend,
                            config_ref,
                            project_root_ref,
                        )
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|h| h.join().expect("language chain thread panicked"))
                .collect()
        });

        // Step 5: put executors back, collect indexed nodes, propagate any Err.
        let mut all_indexed: Vec<(usize, ExecutedNode)> = Vec::new();
        for result in chain_results {
            let (lang, exec, nodes) = result?;
            if let Some(exec) = exec {
                self.executor_manager.put_back(lang, exec);
            }
            all_indexed.extend(nodes);
        }

        // Step 6: restore document order.
        all_indexed.sort_by_key(|(i, _)| *i);
        Ok(all_indexed.into_iter().map(|(_, n)| n).collect())
    }
}

// ---------------------------------------------------------------------------
// Pass 2 helpers — language chain execution
// ---------------------------------------------------------------------------

/// Group planned nodes by language, preserving their original document indices.
fn group_by_language(planned: Vec<PlannedNode>) -> HashMap<String, Vec<(usize, PlannedNode)>> {
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
fn run_language_chain(
    lang: String,
    nodes: Vec<(usize, PlannedNode)>,
    mut exec: Option<Box<dyn KnotExecutor>>,
    cache: Arc<Mutex<Cache>>,
    mut snapshot_manager: SnapshotManager,
    backend: &TypstBackend,
    config: &Config,
    project_root: &Path,
) -> Result<(String, Option<Box<dyn KnotExecutor>>, Vec<(usize, ExecutedNode)>)> {
    use chunk_processor::format_output;

    let mut indexed = Vec::with_capacity(nodes.len());
    let mut broken = false;

    for (doc_idx, pn) in nodes {
        let is_chunk = matches!(&pn.node, ExecutableNode::Chunk(_));
        let source_line = match &pn.node {
            ExecutableNode::Chunk(c) => (c.range.start.line + 1) as u32,
            ExecutableNode::InlineExpr(_) => 0,
        };

        let state = if broken {
            ChunkExecutionState::Inert
        } else {
            ChunkExecutionState::Ready
        };

        // Snapshot update is inlined into each successful arm to avoid double-matching
        // pn.need, which would confuse the borrow checker about the lifetime of `exec`.
        let (typst_content, errored) = if matches!(state, ChunkExecutionState::Inert) {
            (inert_output(&pn, backend, config), false)
        } else {
            match pn.need {
                ExecutionNeed::CacheHit(ref output) => {
                    let data = pn.chunk_data.as_ref().unwrap();
                    info!("  ✓ {} [cached]", data.name);
                    if let Some(error) = &output.error {
                        broken = true;
                        (
                            format_error_block_for_node(&pn.node, &lang, &error.to_string()),
                            true,
                        )
                    } else {
                        let chunk = match &pn.node {
                            ExecutableNode::Chunk(c) => c,
                            _ => unreachable!(),
                        };
                        let content = format_output(
                            backend,
                            chunk,
                            &data.merged_codly_options,
                            &data.resolved_options,
                            output,
                            &state,
                        );
                        {
                            let cache_guard = cache.lock().unwrap();
                            snapshot_manager.update_after_node(
                                &lang,
                                &pn.hash,
                                &pn.previous_hash,
                                &mut exec,
                                &*cache_guard,
                                project_root,
                            )?;
                        }
                        (content, false)
                    }
                }

                ExecutionNeed::CacheHitInline(ref result) => {
                    info!("  ✓ [cached inline]");
                    let result_clone = result.clone();
                    {
                        let cache_guard = cache.lock().unwrap();
                        snapshot_manager.update_after_node(
                            &lang,
                            &pn.hash,
                            &pn.previous_hash,
                            &mut exec,
                            &*cache_guard,
                            project_root,
                        )?;
                    }
                    (result_clone, false)
                }

                ExecutionNeed::Skip => {
                    let content = skip_output(&pn, backend, &state);
                    {
                        let cache_guard = cache.lock().unwrap();
                        snapshot_manager.update_after_node(
                            &lang,
                            &pn.hash,
                            &pn.previous_hash,
                            &mut exec,
                            &*cache_guard,
                            project_root,
                        )?;
                    }
                    (content, false)
                }

                ExecutionNeed::MustExecute => {
                    match execute_for_node(&pn, &mut exec, &cache, &mut snapshot_manager, project_root) {
                        Ok(output) => {
                            if let Some(error) = &output.error {
                                // Runtime error: persist to cache, then cascade.
                                if let ExecutableNode::Chunk(chunk) = &pn.node {
                                    let data = pn.chunk_data.as_ref().unwrap();
                                    if data.resolved_options.cache {
                                        cache.lock().unwrap().save_error(
                                            chunk.index,
                                            chunk.name.clone(),
                                            chunk.language.clone(),
                                            pn.hash.clone(),
                                            error.clone(),
                                            data.chunk_options.depends.clone(),
                                        )?;
                                    }
                                }
                                broken = true;
                                (
                                    format_error_block_for_node(
                                        &pn.node,
                                        &lang,
                                        &error.to_string(),
                                    ),
                                    true,
                                )
                            } else {
                                // Register freeze objects if declared.
                                if let ExecutableNode::Chunk(chunk) = &pn.node {
                                    if !chunk.options.freeze.is_empty() {
                                        register_freeze_objects(
                                            chunk,
                                            &mut exec,
                                            &cache,
                                            project_root,
                                        )?;
                                    }
                                }
                                // Check freeze contract: cascade like a runtime error if violated.
                                // IMPORTANT: save_result is only called when the contract passes.
                                // A violating chunk must NOT be cached as success — if it were,
                                // the check would be bypassed (CacheHit path) on every subsequent run.
                                match check_freeze_contract(&pn, &mut exec, &cache) {
                                    Err(e) => {
                                        // Hash computation failed: cascade without caching.
                                        broken = true;
                                        (
                                            format_error_block_for_node(
                                                &pn.node,
                                                &lang,
                                                &e.to_string(),
                                            ),
                                            true,
                                        )
                                    }
                                    Ok(Some(violation)) => {
                                        // Contract violated: cache the error so LSP shows full
                                        // details (detailed_message), then cascade.
                                        if let ExecutableNode::Chunk(chunk) = &pn.node {
                                            let data = pn.chunk_data.as_ref().unwrap();
                                            if data.resolved_options.cache {
                                                cache.lock().unwrap().save_error(
                                                    chunk.index,
                                                    chunk.name.clone(),
                                                    chunk.language.clone(),
                                                    pn.hash.clone(),
                                                    violation.clone(),
                                                    data.chunk_options.depends.clone(),
                                                )?;
                                            }
                                        }
                                        broken = true;
                                        (
                                            format_error_block_for_node(
                                                &pn.node,
                                                &lang,
                                                &violation.to_string(),
                                            ),
                                            true,
                                        )
                                    }
                                    Ok(None) => {
                                        // Contract OK: persist result to cache.
                                        if let ExecutableNode::Chunk(chunk) = &pn.node {
                                            let data = pn.chunk_data.as_ref().unwrap();
                                            if data.resolved_options.cache {
                                                cache.lock().unwrap().save_result(
                                                    chunk.index,
                                                    chunk.name.clone(),
                                                    chunk.language.clone(),
                                                    pn.hash.clone(),
                                                    &output,
                                                    data.chunk_options.depends.clone(),
                                                )?;
                                            }
                                        }
                                        {
                                            let cache_guard = cache.lock().unwrap();
                                            snapshot_manager.update_after_node(
                                                &lang,
                                                &pn.hash,
                                                &pn.previous_hash,
                                                &mut exec,
                                                &*cache_guard,
                                                project_root,
                                            )?;
                                        }
                                        let content =
                                            format_executed_node(&pn, &output, backend, &state);
                                        (content, false)
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            broken = true;
                            (
                                format_error_block_for_node(&pn.node, &lang, &e.to_string()),
                                true,
                            )
                        }
                    }
                }
            }
        };

        indexed.push((
            doc_idx,
            ExecutedNode {
                lang: lang.clone(),
                hash: pn.hash,
                source_start: pn.source_start,
                source_end: pn.source_end,
                typst_content,
                is_chunk,
                source_line,
                errored,
            },
        ));
    }

    Ok((lang, exec, indexed))
}

/// Restore session snapshot if needed, then execute the node's code.
///
/// Returns the raw `ExecutionOutput` without formatting.  Only called for
/// `ExecutionNeed::MustExecute` nodes.
///
/// **Does not persist to cache.** Caching is done by the caller (`run_language_chain`)
/// so that chunk results are only saved after all post-execution checks (e.g.
/// freeze contract) have passed.  Saving before those checks would mark a
/// violating chunk as a cache hit, silently bypassing the check on every
/// subsequent run.
fn execute_for_node(
    pn: &PlannedNode,
    exec: &mut Option<Box<dyn KnotExecutor>>,
    cache: &Arc<Mutex<Cache>>,
    snapshot_manager: &mut SnapshotManager,
    project_root: &Path,
) -> Result<ExecutionOutput> {
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
            let data = pn.chunk_data.as_ref().unwrap();
            info!("  ⚙️ {} [executing]", data.name);

            if exec.is_none() {
                return Err(anyhow::anyhow!("Unsupported language: '{}'", pn.lang));
            }

            // Lazy state restoration: lock only for the read, release before executing.
            {
                let cache_guard = cache.lock().unwrap();
                snapshot_manager.restore_if_needed(
                    &pn.lang,
                    &pn.previous_hash,
                    exec,
                    &*cache_guard,
                    project_root,
                )?;
                // cache_guard dropped here
            }

            let graphics_opts = GraphicsOptions {
                width: data.resolved_options.fig_width,
                height: data.resolved_options.fig_height,
                dpi: data.resolved_options.dpi,
                format: data.resolved_options.fig_format.as_str().to_string(),
            };

            // SAFETY: checked is_none() above; restore_if_needed doesn't consume the executor.
            exec.as_deref_mut().unwrap().execute(&chunk.code, &graphics_opts)
        }

        ExecutableNode::InlineExpr(inline) => {
            info!("  ⚙️ `{{{}}} {}` [executing]", inline.language, inline.code);

            let e = exec.as_deref_mut().ok_or_else(|| {
                anyhow::anyhow!("Unsupported language: '{}'", pn.lang)
            })?;

            let result = e
                .execute_inline(&inline.code)
                .context(format!(
                    "Failed to execute inline expression: `{{{}}} {}`",
                    inline.language, inline.code
                ))?;

            let resolved = inline.options.resolve();
            let final_result = match resolved.show {
                crate::parser::Show::Output | crate::parser::Show::Both => result,
                crate::parser::Show::Code | crate::parser::Show::None => String::new(),
            };

            cache.lock().unwrap().save_inline_result(pn.hash.clone(), &final_result)?;

            Ok(ExecutionOutput {
                result: ExecutionResult::Text(final_result),
                warnings: vec![],
                error: None,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 3 — Assemble
// ---------------------------------------------------------------------------

/// Interleave formatted node outputs with the verbatim source text between nodes.
fn assemble_pass(executed: &[ExecutedNode], source: &str, source_file: &str) -> String {
    let mut output = String::new();
    let mut last_pos = 0;

    for node in executed {
        if node.source_start > last_pos {
            output.push_str(&source[last_pos..node.source_start]);
        }

        if node.is_chunk {
            output.push_str(&format!(
                "// #KNOT-SYNC source={} line={}\n",
                source_file, node.source_line,
            ));
            output.push_str(&node.typst_content);
            if !node.typst_content.is_empty() && !node.typst_content.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("// END-KNOT-SYNC\n");
        } else {
            output.push_str(&node.typst_content);
        }

        // Advance past the closing fence's trailing newline for chunks.
        last_pos = node.source_end;
        if node.is_chunk && last_pos < source.len() && source.as_bytes()[last_pos] == b'\n' {
            last_pos += 1;
        }
    }

    if last_pos < source.len() {
        output.push_str(&source[last_pos..]);
    }

    output
}

// ---------------------------------------------------------------------------
// Per-node output helpers (called from run_language_chain)
// ---------------------------------------------------------------------------

/// Format the output of a freshly executed node.
fn format_executed_node(
    pn: &PlannedNode,
    output: &ExecutionOutput,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    use chunk_processor::format_output;
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
fn inert_output(pn: &PlannedNode, backend: &TypstBackend, config: &Config) -> String {
    use chunk_processor::{format_output, resolve_options};
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
            let (_, inert_resolved, inert_codly) =
                resolve_options(chunk, config, &ChunkExecutionState::Inert);
            let empty = ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
                error: None,
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
fn skip_output(
    pn: &PlannedNode,
    backend: &TypstBackend,
    state: &ChunkExecutionState,
) -> String {
    use chunk_processor::format_output;
    match &pn.node {
        ExecutableNode::Chunk(chunk) => {
            let data = pn.chunk_data.as_ref().unwrap();
            let empty = ExecutionOutput {
                result: ExecutionResult::Text(String::new()),
                warnings: vec![],
                error: None,
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
fn format_error_block_for_node(node: &ExecutableNode, lang: &str, error_msg: &str) -> String {
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

// ---------------------------------------------------------------------------
// Document node helpers
// ---------------------------------------------------------------------------

/// Collects all executable nodes from the document and sorts them by source position.
fn build_executable_nodes(doc: &Document) -> Vec<ExecutableNode> {
    let mut nodes: Vec<ExecutableNode> = doc
        .chunks
        .iter()
        .map(|c| ExecutableNode::Chunk(c.clone()))
        .chain(doc.inline_exprs.iter().map(|e| ExecutableNode::InlineExpr(e.clone())))
        .collect();
    nodes.sort_by_key(|node| match node {
        ExecutableNode::Chunk(c) => c.start_byte,
        ExecutableNode::InlineExpr(e) => e.start,
    });
    nodes
}

// ---------------------------------------------------------------------------
// Freeze object helpers
// ---------------------------------------------------------------------------

/// Returns the composite cache key for a freeze object: `"lang::varname"`.
///
/// Using a composite key prevents name collisions when R and Python both
/// declare a freeze object with the same variable name.
fn freeze_key(lang: &str, name: &str) -> String {
    format!("{}::{}", lang, name)
}

/// Saves all freeze objects declared by a chunk to the object cache.
///
/// `exec` must be `Some` when this is called (freeze objects only arise from
/// successfully executed chunks, so an executor is always present).
fn register_freeze_objects(
    chunk: &Chunk,
    exec: &mut Option<Box<dyn KnotExecutor>>,
    cache: &Arc<Mutex<Cache>>,
    project_root: &Path,
) -> Result<()> {
    let exec = exec
        .as_deref_mut()
        .expect("executor must be present when registering freeze objects");
    let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

    for obj_name in &chunk.options.freeze {
        let obj_hash = exec
            .hash_object(obj_name)
            .context(format!("Failed to hash freeze object '{}'", obj_name))?;

        exec.save_constant(obj_name, &obj_hash, &cache_dir)
            .context(format!("Failed to save freeze object '{}'", obj_name))?;

        let ext = exec.object_extension();
        let object_path = cache_dir
            .join("objects")
            .join(format!("{}.{}", obj_hash, ext));
        let size_bytes = std::fs::metadata(&object_path)?.len();

        let key = freeze_key(&chunk.language, obj_name);
        cache.lock().unwrap().metadata.freeze_objects.insert(
            key,
            FreezeObjectInfo {
                name: obj_name.clone(),
                hash: obj_hash,
                size_bytes,
                language: chunk.language.clone(),
                created_in_chunk: chunk_name.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        log::info!(
            "🔒 Freeze object '{}' ({}) declared in chunk '{}'",
            obj_name,
            chunk.language,
            chunk_name
        );
    }
    Ok(())
}

/// Checks that no freeze object for `pn`'s language was mutated during chunk execution.
///
/// Called after each successful MustExecute node, before saving the snapshot.
///
/// Returns:
/// - `Ok(None)` — all freeze contracts satisfied, execution can proceed
/// - `Ok(Some(error))` — a freeze object was mutated; `error` is a [`RuntimeError`]
///   whose `to_string()` gives a short PDF message and whose
///   `detailed_message()` gives the full LSP diagnostic
/// - `Err(e)` — the hash computation itself failed (propagated to the caller)
fn check_freeze_contract(
    pn: &PlannedNode,
    exec: &mut Option<Box<dyn KnotExecutor>>,
    cache: &Arc<Mutex<Cache>>,
) -> Result<Option<RuntimeError>> {
    // Collect needed data while holding the lock briefly, then release before calling executor.
    let freeze_entries: Vec<FreezeObjectInfo> = {
        let cache_guard = cache.lock().unwrap();
        cache_guard
            .metadata
            .freeze_objects
            .values()
            .filter(|info| info.language == pn.lang)
            .cloned()
            .collect()
    };

    if freeze_entries.is_empty() {
        return Ok(None);
    }

    let exec = match exec.as_deref_mut() {
        Some(e) => e,
        None => return Ok(None),
    };

    let chunk_name = match &pn.node {
        ExecutableNode::Chunk(c) => c.name.as_deref().unwrap_or("unnamed"),
        ExecutableNode::InlineExpr(_) => "inline",
    };

    for info in &freeze_entries {
        let current_hash = exec
            .hash_object(&info.name)
            .context(format!("Failed to hash freeze object '{}'", info.name))?;

        if current_hash != info.hash {
            let error = RuntimeError {
                message: Some(format!(
                    "Freeze contract violated: object '{}' ({}) was modified in chunk '{}'",
                    info.name, info.language, chunk_name
                )),
                call: None,
                line: None,
                traceback: vec![
                    format!(
                        "Object '{}' was frozen in chunk '{}'",
                        info.name, info.created_in_chunk
                    ),
                    format!("Expected hash : {}", info.hash),
                    format!("Current hash  : {}", current_hash),
                    String::from("Frozen objects must not be mutated after declaration."),
                ],
            };
            return Ok(Some(error));
        }
    }

    Ok(None)
}

#[cfg(test)]
pub(super) mod test_helpers {
    use crate::cache::Cache;
    use crate::executors::ExecutorManager;
    use tempfile::TempDir;

    pub fn setup_test_cache() -> (TempDir, Cache) {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, cache)
    }

    pub fn setup_test_manager() -> (TempDir, ExecutorManager) {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExecutorManager::new(temp_dir.path().to_path_buf());
        (temp_dir, manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Document;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_test_compiler() -> (TempDir, Compiler) {
        let temp_dir = TempDir::new().unwrap();
        let knot_file = temp_dir.path().join("test.knot");
        std::fs::write(&knot_file, "").unwrap();
        let compiler = Compiler::new(&knot_file).unwrap();
        (temp_dir, compiler)
    }

    fn make_executed_node(
        source_start: usize,
        source_end: usize,
        typst_content: &str,
        is_chunk: bool,
        source_line: u32,
    ) -> ExecutedNode {
        ExecutedNode {
            lang: "r".to_string(),
            hash: "abc123".to_string(),
            source_start,
            source_end,
            typst_content: typst_content.to_string(),
            is_chunk,
            source_line,
            errored: false,
        }
    }

    // -----------------------------------------------------------------------
    // assemble_pass — pure function, no R/Python required
    // -----------------------------------------------------------------------

    #[test]
    fn test_assemble_no_nodes_returns_source_unchanged() {
        let source = "Hello, Typst!";
        let result = assemble_pass(&[], source, "test.knot");
        assert_eq!(result, source);
    }

    #[test]
    fn test_assemble_inline_inserted_verbatim() {
        // Source: "prefix INLINE suffix"
        // The inline node occupies bytes 7..13 ("INLINE").
        let source = "prefix INLINE suffix";
        let node = make_executed_node(7, 13, "42", false, 0);
        let result = assemble_pass(&[node], source, "test.knot");
        assert_eq!(result, "prefix 42 suffix");
    }

    #[test]
    fn test_assemble_chunk_has_sync_markers() {
        // Source: a fenced code block. start_byte points to the opening fence,
        // end_byte points just past the closing fence (not including the newline).
        let source = "```{r}\nx <- 1\n```\nafter";
        // Chunk spans the entire fenced block; trailing newline at byte 18 gets consumed.
        let chunk_end = source.find("```\nafter").unwrap() + 3; // points to the `\n` after closing ```
        let node = make_executed_node(0, chunk_end, "#code-chunk()", true, 1);
        let result = assemble_pass(&[node], source, "test.knot");
        assert!(
            result.contains("// #KNOT-SYNC source=test.knot line=1\n"),
            "Missing opening sync marker, got:\n{result}"
        );
        assert!(
            result.contains("// END-KNOT-SYNC\n"),
            "Missing closing sync marker, got:\n{result}"
        );
        assert!(
            result.contains("#code-chunk()"),
            "Missing chunk content, got:\n{result}"
        );
    }

    #[test]
    fn test_assemble_chunk_content_without_newline_gets_one() {
        let source = "```{r}\ncode\n```\nrest";
        let chunk_end = source.find("```\nrest").unwrap() + 3;
        // typst_content does NOT end with '\n'
        let node = make_executed_node(0, chunk_end, "no-newline", true, 1);
        let result = assemble_pass(&[node], source, "test.knot");
        assert!(
            result.contains("no-newline\n// END-KNOT-SYNC"),
            "Expected newline inserted before END marker, got:\n{result}"
        );
    }

    #[test]
    fn test_assemble_chunk_content_with_newline_not_doubled() {
        let source = "```{r}\ncode\n```\nrest";
        let chunk_end = source.find("```\nrest").unwrap() + 3;
        // typst_content ends with '\n' — must NOT add another
        let node = make_executed_node(0, chunk_end, "has-newline\n", true, 1);
        let result = assemble_pass(&[node], source, "test.knot");
        assert!(
            result.contains("has-newline\n// END-KNOT-SYNC"),
            "Newline should not be doubled before END marker, got:\n{result}"
        );
        assert!(
            !result.contains("has-newline\n\n// END-KNOT-SYNC"),
            "Newline was doubled, got:\n{result}"
        );
    }

    #[test]
    fn test_assemble_multiple_nodes_interleaved() {
        // "AAA ```{r}\ncode\n``` BBB `r 1+1` CCC"
        //   0   4         17  21   25       30
        let source = "AAA ```{r}\ncode\n``` BBB `r 1+1` CCC";
        // chunk: bytes 4..19 (```{r}\ncode\n```)
        let chunk_end = source.find("``` BBB").unwrap() + 3;
        // inline: bytes 24..32 (`r 1+1`)
        let inline_start = source.find("`r 1+1`").unwrap();
        let inline_end = inline_start + "`r 1+1`".len();

        let chunk = make_executed_node(4, chunk_end, "#chunk()", true, 1);
        let inline = make_executed_node(inline_start, inline_end, "2", false, 0);
        let result = assemble_pass(&[chunk, inline], source, "test.knot");

        assert!(result.starts_with("AAA "), "Prefix 'AAA ' missing");
        assert!(
            result.contains("// #KNOT-SYNC"),
            "Chunk sync marker missing"
        );
        assert!(result.contains("BBB "), "Inter-node text 'BBB ' missing");
        assert!(result.contains("2"), "Inline result missing");
        assert!(result.ends_with(" CCC"), "Suffix ' CCC' missing");
    }

    // -----------------------------------------------------------------------
    // Integration: Inert cascade — uses unsupported language (no R/Python)
    // -----------------------------------------------------------------------

    #[test]
    fn test_inert_cascade_on_unsupported_language() {
        let source =
            "```{unsupported_lang}\ncode1\n```\n\n```{unsupported_lang}\ncode2\n```\n".to_string();
        let doc = Document::parse(source);
        let (_temp_dir, mut compiler) = make_test_compiler();

        let result = compiler
            .compile(&doc, "test.knot")
            .expect("compile() must succeed even when a language executor is unavailable");

        // First chunk: produces an execution error block.
        assert!(
            result.contains("Execution Error"),
            "Expected error block for first chunk, got:\n{result}"
        );

        // Second chunk: should be rendered as inert (cascade).
        assert!(
            result.contains("is-inert: true"),
            "Expected inert marker for second chunk (cascade), got:\n{result}"
        );
    }
}
