//! Three-pass compilation pipeline for `.knot` documents.
//!
//! The [`Compiler`] struct drives Plan → Execute → Assemble.
//! See the crate-level documentation for a full description of the pipeline.

use crate::cache::Cache;
use crate::config::Config;
use crate::executors::{ExecutorManager, KnotExecutor};
use crate::parser::ast::{Chunk, Document, InlineExpr};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod formatters;
pub mod pipeline;
pub mod snapshot_manager;
/// Bidirectional source ↔ PDF navigation via `#KNOT-SYNC` markers.
pub mod sync;

mod execution;
mod freeze;
mod node_output;
mod options;

pub use execution::ProgressEvent;
pub use pipeline::{
    ChunkExecutionState, ExecutedNode, ExecutionNeed, PlannedNode, PlannedNodeKind,
};

/// Controls how `MustExecute` chunks are rendered in Phase 0 (before execution).
#[derive(Clone, Copy)]
pub enum Phase0Mode {
    /// A full compile is in progress (`do_compile`): show orange border on all
    /// `MustExecute` chunks to signal that execution is underway.
    Pending,
    /// The user edited the file without triggering a compile (`do_phase0_only`):
    /// show amber (strong) on the first `MustExecute` per language chain and
    /// amber (muted) on downstream hash-invalidated chunks.
    Modified,
}

use crate::backend::TypstBackend;
use crate::compiler::pipeline::ChunkPlanData;
use crate::get_cache_dir;
use anyhow::Result;
use log::info;

use execution::{ChainOutput, group_by_language, run_language_chain};
use node_output::{format_error_block_for_node, format_executed_node, skip_output};
use options::{compute_hash, resolve_options};

/// Represents a node in the document that can be executed.
pub enum ExecutableNode {
    /// A fenced code chunk.
    Chunk(Box<Chunk>),
    /// An inline expression `` `{lang} code` ``.
    InlineExpr(InlineExpr),
}

/// The compilation engine for a single `.knot` document.
///
/// Holds the executor pool, resolved config, project root and cache directory.
/// Create via [`Compiler::new`], then call [`Compiler::plan_and_partial`] and
/// [`Compiler::execute_and_assemble_streaming`] for progressive compilation, or use the
/// project-level helpers in [`crate::project`] for full-project builds.
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

    /// Plan phase + Phase-0 assembly for progressive compilation.
    ///
    /// Runs Pass 1 (plan) and immediately assembles a partial Typst string where
    /// cached/skipped nodes are rendered with their real content and `MustExecute`
    /// nodes are rendered as pending placeholders.
    ///
    /// Returns `(planned, cache, phase0_typ)`:
    /// - `planned`: the full planning result, needed by [`execute_and_assemble_streaming`].
    /// - `cache`:   shared cache handle (same instance used by the execute pass).
    /// - `phase0_typ`: a Typst string ready for immediate preview.
    pub fn plan_and_partial(
        &mut self,
        doc: &Document,
        source_file: &str,
        mode: Phase0Mode,
    ) -> Result<(Vec<PlannedNode>, Arc<Mutex<Cache>>, String)> {
        let cache = Arc::new(Mutex::new(Cache::new(self.cache_dir.clone())?));
        let backend = TypstBackend::new();

        let nodes = build_executable_nodes(doc);
        let planned = self.plan_pass(nodes, &cache)?;

        let phase0_typ = assemble_partial(&planned, &doc.source, source_file, &backend, mode);
        Ok((planned, cache, phase0_typ))
    }

    /// Execute phase with per-chunk streaming + final assembly.
    ///
    /// Runs Pass 2 (execute) and Pass 3 (assemble). After each node completes,
    /// a [`ProgressEvent`] is sent via `progress` so the caller can render
    /// incremental preview updates without waiting for all chunks to finish.
    ///
    /// Saves the cache after assembly. Call this after [`plan_and_partial`].
    pub fn execute_and_assemble_streaming(
        &mut self,
        planned: Vec<PlannedNode>,
        cache: Arc<Mutex<Cache>>,
        source: &str,
        source_file: &str,
        progress: Option<std::sync::mpsc::Sender<ProgressEvent>>,
    ) -> Result<String> {
        let backend = TypstBackend::new();
        let executed = self.execute_pass(planned, Arc::clone(&cache), &backend, progress)?;
        let typst_output = assemble_pass(&executed, source, source_file);
        cache.lock().unwrap().save_metadata()?;
        Ok(typst_output)
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
        let executed = self.execute_pass(planned, Arc::clone(&cache), &backend, None)?;

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
        // Lock once for the entire planning pass (synchronous, no contention).
        let cache = cache.lock().unwrap();

        let mut planned = Vec::with_capacity(nodes.len());
        let mut last_hash_per_lang: HashMap<String, String> = HashMap::new();

        for node in nodes {
            let (lang, source_start, source_end) = match &node {
                ExecutableNode::Chunk(c) => (c.language.clone(), c.start_byte, c.end_byte),
                ExecutableNode::InlineExpr(e) => (e.language.clone(), e.start, e.end),
            };
            let previous_hash = last_hash_per_lang.get(&lang).cloned().unwrap_or_default();

            let (hash, need, kind) = match node {
                ExecutableNode::Chunk(chunk) => {
                    let (chunk_options, resolved_options, merged_codly_options) =
                        resolve_options(&chunk, &self.config, &ChunkExecutionState::Ready);
                    let name = chunk
                        .label
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
                    let kind = PlannedNodeKind::Chunk {
                        node: chunk,
                        data: Box::new(ChunkPlanData {
                            resolved_options,
                            chunk_options,
                            merged_codly_options,
                            name,
                        }),
                    };
                    (hash, need, kind)
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
                    (hash, need, PlannedNodeKind::Inline { node: inline })
                }
            };

            last_hash_per_lang.insert(lang.clone(), hash.clone());
            planned.push(PlannedNode {
                kind,
                lang,
                hash,
                previous_hash,
                source_start,
                source_end,
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
    ///
    /// When `progress` is `Some`, each completed node emits a [`ProgressEvent`].
    fn execute_pass(
        &mut self,
        planned: Vec<PlannedNode>,
        cache: Arc<Mutex<Cache>>,
        backend: &TypstBackend,
        progress: Option<std::sync::mpsc::Sender<ProgressEvent>>,
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
        let chain_data = groups
            .into_iter()
            .map(|(lang, nodes)| {
                let exec = chain_executors.remove(&lang);
                (lang, nodes, exec)
            })
            .collect::<Vec<_>>();

        // Step 4: run each language chain in its own OS thread.
        type ChainResult = Result<ChainOutput>;

        // Reborrow as references so closures can copy them (references are Copy).
        let config_ref = &config;
        let project_root_ref = &project_root;

        let chain_results: Vec<ChainResult> = std::thread::scope(|s| {
            let handles: Vec<_> = chain_data
                .into_iter()
                .map(|(lang, nodes, exec)| {
                    let cache = Arc::clone(&cache);
                    let chain_progress = progress.clone();
                    s.spawn(move || {
                        run_language_chain(
                            lang,
                            nodes,
                            exec,
                            cache,
                            backend,
                            config_ref,
                            project_root_ref,
                            chain_progress,
                        )
                    })
                })
                .collect();

            handles
                .into_iter()
                // Re-raise the original panic payload rather than wrapping it in a new
                // string via `.expect()`. This preserves the panic location and backtrace.
                .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                .collect()
        });

        // Step 5: put executors back, collect indexed nodes, propagate any Err.
        // Always put back ALL executors before propagating an error — a `?` on the
        // first Err would skip the remaining iterations and silently drop live executors.
        let mut all_indexed: Vec<(usize, ExecutedNode)> = Vec::new();
        let mut first_error: Option<anyhow::Error> = None;
        for result in chain_results {
            match result {
                Ok((lang, exec, nodes)) => {
                    if let Some(exec) = exec {
                        self.executor_manager.put_back(lang, exec);
                    }
                    all_indexed.extend(nodes);
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }
        if let Some(e) = first_error {
            return Err(e);
        }

        // Step 6: restore document order.
        all_indexed.sort_by_key(|(i, _)| *i);
        Ok(all_indexed.into_iter().map(|(_, n)| n).collect())
    }
}

// ---------------------------------------------------------------------------
// Pass 3 — Assemble
// ---------------------------------------------------------------------------

/// Interleave formatted node outputs with the verbatim source text between nodes.
pub fn assemble_pass(executed: &[ExecutedNode], source: &str, source_file: &str) -> String {
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
// Progressive compilation helpers
// ---------------------------------------------------------------------------

/// Convert planned nodes to partial executed nodes without running any code.
///
/// For each [`PlannedNode`]:
/// - `Skip` / `CacheHit(Success)` → rendered with real content.
/// - `CacheHit(RuntimeError)`     → rendered as an error block; language marked Inert.
/// - `MustExecute`                → rendered according to `mode`:
///   - [`Phase0Mode::Pending`]   → orange border (`is-pending`) for all.
///   - [`Phase0Mode::Modified`]  → amber strong (`is-modified`) for the first in
///     each language chain, amber muted (`is-modified-cascade`) for the rest.
///
/// The returned `Vec<ExecutedNode>` can be fed directly to [`assemble_pass`].
/// In a streaming compilation loop, replace individual entries as [`ProgressEvent`]s
/// arrive, then call [`assemble_pass`] again to get an updated preview.
pub fn planned_to_partial_nodes(
    planned: &[PlannedNode],
    backend: &TypstBackend,
    mode: Phase0Mode,
) -> Vec<ExecutedNode> {
    // Track which languages have a cached runtime error so that subsequent
    // MustExecute nodes in the same language chain are rendered as Inert.
    let mut inert_langs: HashSet<String> = HashSet::new();
    // Track which languages have already seen a MustExecute node (Modified mode):
    // the first MustExecute per language = direct edit (Modified),
    // subsequent ones = hash cascade (ModifiedCascade).
    let mut must_execute_langs: HashSet<String> = HashSet::new();
    let mut result = Vec::with_capacity(planned.len());

    for pn in planned {
        let (is_chunk, source_line) = match &pn.kind {
            PlannedNodeKind::Chunk { node, .. } => (true, (node.range.start.line + 1) as u32),
            PlannedNodeKind::Inline { .. } => (false, 0),
        };

        let (typst_content, errored) = match &pn.need {
            ExecutionNeed::Skip => {
                must_execute_langs.remove(&pn.lang);
                (skip_output(pn, backend, &ChunkExecutionState::Ready), false)
            }
            ExecutionNeed::CacheHit(crate::executors::ExecutionAttempt::Success(output)) => {
                must_execute_langs.remove(&pn.lang);
                (
                    format_executed_node(pn, output, backend, &ChunkExecutionState::Ready),
                    false,
                )
            }
            ExecutionNeed::CacheHit(crate::executors::ExecutionAttempt::RuntimeError(e)) => {
                // Mark this language as errored so downstream MustExecute nodes
                // in the same chain are shown as Inert in Phase 0.
                inert_langs.insert(pn.lang.clone());
                must_execute_langs.remove(&pn.lang);
                (
                    format_error_block_for_node(&pn.kind, &pn.lang, &e.to_string()),
                    true,
                )
            }
            ExecutionNeed::CacheHitInline(text) => {
                must_execute_langs.remove(&pn.lang);
                (text.clone(), false)
            }
            ExecutionNeed::MustExecute => {
                if inert_langs.contains(&pn.lang) {
                    // Upstream error cached for this language → will be Inert.
                    (skip_output(pn, backend, &ChunkExecutionState::Inert), false)
                } else {
                    match mode {
                        Phase0Mode::Pending => (
                            skip_output(pn, backend, &ChunkExecutionState::Pending),
                            false,
                        ),
                        Phase0Mode::Modified => {
                            if must_execute_langs.contains(&pn.lang) {
                                // Subsequent MustExecute in the same chain = cascade.
                                (
                                    skip_output(pn, backend, &ChunkExecutionState::ModifiedCascade),
                                    false,
                                )
                            } else {
                                // First MustExecute for this language = direct edit.
                                must_execute_langs.insert(pn.lang.clone());
                                (
                                    skip_output(pn, backend, &ChunkExecutionState::Modified),
                                    false,
                                )
                            }
                        }
                    }
                }
            }
        };

        result.push(ExecutedNode {
            lang: pn.lang.clone(),
            hash: pn.hash.clone(),
            source_start: pn.source_start,
            source_end: pn.source_end,
            typst_content,
            is_chunk,
            source_line,
            errored,
        });
    }

    result
}

/// Assemble a Phase-0 Typst string from planned nodes (no execution).
///
/// Equivalent to `assemble_pass(&planned_to_partial_nodes(planned, backend, mode), source, source_file)`.
pub fn assemble_partial(
    planned: &[PlannedNode],
    source: &str,
    source_file: &str,
    backend: &TypstBackend,
    mode: Phase0Mode,
) -> String {
    let partial = planned_to_partial_nodes(planned, backend, mode);
    assemble_pass(&partial, source, source_file)
}

// ---------------------------------------------------------------------------
// Document node helpers
// ---------------------------------------------------------------------------

/// Collects all executable nodes from the document and sorts them by source position.
fn build_executable_nodes(doc: &Document) -> Vec<ExecutableNode> {
    let mut nodes: Vec<ExecutableNode> = doc
        .chunks
        .iter()
        .map(|c| ExecutableNode::Chunk(Box::new(c.clone())))
        .chain(
            doc.inline_exprs
                .iter()
                .map(|e| ExecutableNode::InlineExpr(e.clone())),
        )
        .collect();
    nodes.sort_by_key(|node| match node {
        ExecutableNode::Chunk(c) => c.start_byte,
        ExecutableNode::InlineExpr(e) => e.start,
    });
    nodes
}

#[cfg(test)]
pub(super) mod test_helpers {
    /// Creates a minimal test chunk for use in unit tests.
    pub fn create_test_chunk(
        language: &str,
        code: &str,
        name: Option<String>,
        eval: bool,
    ) -> crate::parser::Chunk {
        use crate::parser::{ChunkOptions, Position, Range};
        let dummy_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };
        crate::parser::Chunk {
            index: 0,
            language: language.to_string(),
            code: code.to_string(),
            label: name,
            base_indentation: String::new(),
            options: ChunkOptions {
                eval: Some(eval),
                ..Default::default()
            },
            codly_options: std::collections::HashMap::new(),
            errors: vec![],
            range: dummy_range.clone(),
            code_range: dummy_range,
            start_byte: 0,
            end_byte: 0,
            code_start_byte: 0,
            code_end_byte: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Document;
    use insta::assert_snapshot;
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

    #[test]
    fn test_assemble_pass_full_output_snapshot() {
        // Source: prose, then a chunk, then an inline expression.
        let source = "= Intro\n\n```{r}\nx <- 1\n```\n\nResult: `r x`\n";
        let chunk_start = source.find("```{r}").unwrap();
        let chunk_end = source.find("```\n\nResult").unwrap() + 3;
        let inline_start = source.find("`r x`").unwrap();
        let inline_end = inline_start + "`r x`".len();

        let chunk_node = make_executed_node(
            chunk_start,
            chunk_end,
            "#code-chunk(lang: \"r\", code: [```r\nx <- 1```])",
            true,
            3,
        );
        let inline_node = make_executed_node(inline_start, inline_end, "1", false, 0);
        let result = assemble_pass(&[chunk_node, inline_node], source, "test.knot");
        assert_snapshot!(result);
    }

    // -----------------------------------------------------------------------
    // compute_hash
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_hash_consistency() {
        let chunk = test_helpers::create_test_chunk("r", "x <- 1", None, false);
        let hash1 = compute_hash(&chunk.code, &chunk.options, "prev_hash").unwrap();
        let hash2 = compute_hash(&chunk.code, &chunk.options, "prev_hash").unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_changes_with_code() {
        let chunk1 = test_helpers::create_test_chunk("r", "x <- 1", None, false);
        let chunk2 = test_helpers::create_test_chunk("r", "x <- 2", None, false);
        let hash1 = compute_hash(&chunk1.code, &chunk1.options, "prev").unwrap();
        let hash2 = compute_hash(&chunk2.code, &chunk2.options, "prev").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_changes_with_previous() {
        let chunk = test_helpers::create_test_chunk("r", "x <- 1", None, false);
        let hash1 = compute_hash(&chunk.code, &chunk.options, "prev_1").unwrap();
        let hash2 = compute_hash(&chunk.code, &chunk.options, "prev_2").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_includes_dependencies() {
        use std::fs;
        let temp = TempDir::new().unwrap();
        let dep1 = temp.path().join("dep1.txt");
        let dep2 = temp.path().join("dep2.txt");
        fs::write(&dep1, "content1").unwrap();
        fs::write(&dep2, "content2").unwrap();

        let mut chunk = test_helpers::create_test_chunk("r", "x <- 1", None, false);
        chunk.options.depends = vec![dep1];
        let hash1 = compute_hash(&chunk.code, &chunk.options, "prev").unwrap();

        chunk.options.depends = vec![dep2];
        let hash2 = compute_hash(&chunk.code, &chunk.options, "prev").unwrap();

        assert_ne!(hash1, hash2);
    }

    // -----------------------------------------------------------------------
    // resolve_options
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_options_language_specific_defaults() {
        use crate::config::{ChunkDefaults, Config};
        let mut chunk = test_helpers::create_test_chunk("r", "x <- 1", None, true);
        chunk.options.show = None; // let defaults apply

        let config = Config {
            r_chunks: Some(ChunkDefaults {
                show: Some(crate::parser::Show::Output),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (_, resolved, _) = resolve_options(&chunk, &config, &ChunkExecutionState::Ready);
        assert_eq!(resolved.show, crate::parser::Show::Output);
    }

    #[test]
    fn test_resolve_options_chunk_overrides_language_defaults() {
        use crate::config::{ChunkDefaults, Config};
        let mut chunk = test_helpers::create_test_chunk("r", "x <- 1", None, true);
        chunk.options.show = Some(crate::parser::Show::Both); // explicit chunk option

        let config = Config {
            r_chunks: Some(ChunkDefaults {
                show: Some(crate::parser::Show::Output),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (_, resolved, _) = resolve_options(&chunk, &config, &ChunkExecutionState::Ready);
        assert_eq!(resolved.show, crate::parser::Show::Both);
    }

    #[test]
    fn test_resolve_options_global_fallback() {
        use crate::config::{ChunkDefaults, Config};
        let mut chunk = test_helpers::create_test_chunk("python", "x = 1", None, true);
        chunk.options.show = None;

        let config = Config {
            chunk_defaults: ChunkDefaults {
                show: Some(crate::parser::Show::Output),
                ..Default::default()
            },
            ..Default::default()
        };
        let (_, resolved, _) = resolve_options(&chunk, &config, &ChunkExecutionState::Ready);
        assert_eq!(resolved.show, crate::parser::Show::Output);
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
