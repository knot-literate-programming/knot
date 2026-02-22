use crate::cache::FreezeObjectInfo;
use crate::config::Config;
use crate::executors::{ExecutionOutput, ExecutionResult, ExecutorManager, GraphicsOptions};
use crate::parser::ast::{Chunk, Document, InlineExpr};
use std::collections::HashMap;
use std::path::Path;
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
pub enum ExecutableNode<'a> {
    Chunk(&'a Chunk),
    InlineExpr(&'a InlineExpr),
}

use crate::backend::TypstBackend;
use crate::cache::Cache;
use crate::compiler::pipeline::ChunkPlanData;
use crate::compiler::snapshot_manager::SnapshotManager;
use crate::defaults::Defaults;
use crate::get_cache_dir;
use anyhow::{Context, Result};
use log::info;
use std::collections::HashSet;
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
        let mut cache = Cache::new(self.cache_dir.clone())?;
        let backend = TypstBackend::new();
        let mut snapshot_manager = SnapshotManager::new();

        let nodes = build_executable_nodes(doc);
        info!("🔧 Processing {} executable nodes...", nodes.len());

        // Pass 1: resolve options, compute hashes, check cache — no code executed.
        let planned = self.plan_pass(nodes, &mut cache)?;

        // Pass 2: execute pending nodes, format output, handle error cascade.
        let executed = self.execute_pass(planned, &mut cache, &mut snapshot_manager, &backend)?;

        // Pass 3: interleave node outputs with source text.
        let typst_output = assemble_pass(&executed, &doc.source, source_file);

        info!("✓ All nodes processed.");
        cache.save_metadata()?;

        Ok(typst_output)
    }

    // -----------------------------------------------------------------------
    // Pass 1 — Plan
    // -----------------------------------------------------------------------

    /// For every node: resolve options, compute hash, check cache.
    /// Returns a `PlannedNode` for each node — no code is executed.
    fn plan_pass<'a>(
        &mut self,
        nodes: Vec<ExecutableNode<'a>>,
        cache: &mut Cache,
    ) -> Result<Vec<PlannedNode<'a>>> {
        use chunk_processor::{compute_hash, resolve_options};

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
    // Pass 2 — Execute
    // -----------------------------------------------------------------------

    /// Execute pending nodes, format all outputs, propagate the Inert cascade on error.
    fn execute_pass<'a>(
        &mut self,
        planned: Vec<PlannedNode<'a>>,
        cache: &mut Cache,
        snapshot_manager: &mut SnapshotManager,
        backend: &TypstBackend,
    ) -> Result<Vec<ExecutedNode>> {
        use chunk_processor::format_output;

        let mut executed = Vec::with_capacity(planned.len());
        let mut broken_languages: HashSet<String> = HashSet::new();

        for pn in planned {
            let is_chunk = matches!(&pn.node, ExecutableNode::Chunk(_));
            let source_line = match &pn.node {
                ExecutableNode::Chunk(c) => (c.range.start.line + 1) as u32,
                ExecutableNode::InlineExpr(_) => 0,
            };

            // Determine effective execution state for this node.
            let state = if broken_languages.contains(&pn.lang) {
                ChunkExecutionState::Inert
            } else {
                ChunkExecutionState::Ready
            };

            let (typst_content, errored) = if matches!(state, ChunkExecutionState::Inert) {
                // Language is broken — render as inert (no execution).
                (inert_output(&pn, backend, &self.config), false)
            } else {
                match pn.need {
                    ExecutionNeed::CacheHit(ref output) => {
                        let data = pn.chunk_data.as_ref().unwrap();
                        info!("  ✓ {} [cached]", data.name);
                        if let Some(error) = &output.error {
                            broken_languages.insert(pn.lang.clone());
                            (
                                format_error_block_for_node(&pn.node, &pn.lang, &error.to_string()),
                                true,
                            )
                        } else {
                            let chunk = match &pn.node {
                                ExecutableNode::Chunk(c) => c,
                                _ => unreachable!(),
                            };
                            (
                                format_output(
                                    backend,
                                    chunk,
                                    &data.merged_codly_options,
                                    &data.resolved_options,
                                    output,
                                    &state,
                                ),
                                false,
                            )
                        }
                    }

                    ExecutionNeed::CacheHitInline(ref result) => {
                        info!("  ✓ [cached inline]");
                        (result.clone(), false)
                    }

                    ExecutionNeed::Skip => {
                        // eval = false: format with empty output.
                        (skip_output(&pn, backend, &state), false)
                    }

                    ExecutionNeed::MustExecute => {
                        match self.run_node(&pn, cache, snapshot_manager) {
                            Ok(output) => {
                                if let Some(error) = &output.error {
                                    broken_languages.insert(pn.lang.clone());
                                    (
                                        format_error_block_for_node(
                                            &pn.node,
                                            &pn.lang,
                                            &error.to_string(),
                                        ),
                                        true,
                                    )
                                } else {
                                    // Register freeze objects if declared.
                                    if let ExecutableNode::Chunk(chunk) = &pn.node
                                        && !chunk.options.freeze.is_empty()
                                    {
                                        register_freeze_objects(
                                            chunk,
                                            &mut self.executor_manager,
                                            cache,
                                            &self.project_root,
                                        )?;
                                    }
                                    // Check freeze contract: error if any freeze object was mutated.
                                    check_freeze_contract(&pn, &mut self.executor_manager, cache)?;
                                    snapshot_manager.update_after_node(
                                        &pn.lang,
                                        &pn.hash,
                                        &pn.previous_hash,
                                        &mut self.executor_manager,
                                        cache,
                                        &self.project_root,
                                    )?;
                                    let content =
                                        format_executed_node(&pn, &output, backend, &state);
                                    (content, false)
                                }
                            }
                            Err(e) => {
                                broken_languages.insert(pn.lang.clone());
                                (
                                    format_error_block_for_node(&pn.node, &pn.lang, &e.to_string()),
                                    true,
                                )
                            }
                        }
                    }
                }
            };

            // For non-error, non-inert, non-MustExecute nodes: update snapshot state.
            if !errored && !matches!(state, ChunkExecutionState::Inert) {
                match &pn.need {
                    ExecutionNeed::CacheHit(_)
                    | ExecutionNeed::CacheHitInline(_)
                    | ExecutionNeed::Skip => {
                        snapshot_manager.update_after_node(
                            &pn.lang,
                            &pn.hash,
                            &pn.previous_hash,
                            &mut self.executor_manager,
                            cache,
                            &self.project_root,
                        )?;
                    }
                    ExecutionNeed::MustExecute => {}
                }
            }

            executed.push(ExecutedNode {
                lang: pn.lang,
                hash: pn.hash,
                source_start: pn.source_start,
                source_end: pn.source_end,
                typst_content,
                is_chunk,
                source_line,
                errored,
            });
        }

        Ok(executed)
    }

    /// Restore session snapshot if needed, then execute the node's code.
    ///
    /// Returns the raw `ExecutionOutput` without formatting.  Only called for
    /// `ExecutionNeed::MustExecute` nodes.
    fn run_node(
        &mut self,
        pn: &PlannedNode<'_>,
        cache: &mut Cache,
        snapshot_manager: &mut SnapshotManager,
    ) -> Result<ExecutionOutput> {
        match &pn.node {
            ExecutableNode::Chunk(chunk) => {
                let data = pn.chunk_data.as_ref().unwrap();
                info!("  ⚙️ {} [executing]", data.name);

                // Lazy state restoration.
                {
                    let sm = &mut *snapshot_manager;
                    let em = &mut self.executor_manager;
                    let c = &*cache;
                    let pr = self.project_root.as_path();
                    sm.restore_if_needed(&pn.lang, &pn.previous_hash, em, c, pr)?;
                }

                let graphics_opts = GraphicsOptions {
                    width: data.resolved_options.fig_width,
                    height: data.resolved_options.fig_height,
                    dpi: data.resolved_options.dpi,
                    format: data.resolved_options.fig_format.as_str().to_string(),
                };

                let output = {
                    let exec = self.executor_manager.get_executor(&pn.lang)?;
                    exec.execute(&chunk.code, &graphics_opts)?
                };

                if data.resolved_options.cache {
                    if let Some(error) = &output.error {
                        cache.save_error(
                            chunk.index,
                            chunk.name.clone(),
                            chunk.language.clone(),
                            pn.hash.clone(),
                            error.clone(),
                            data.chunk_options.depends.clone(),
                        )?;
                    } else {
                        cache.save_result(
                            chunk.index,
                            chunk.name.clone(),
                            chunk.language.clone(),
                            pn.hash.clone(),
                            &output,
                            data.chunk_options.depends.clone(),
                        )?;
                    }
                }

                Ok(output)
            }
            ExecutableNode::InlineExpr(inline) => {
                info!("  ⚙️ `{{{}}} {}` [executing]", inline.language, inline.code);
                let result = self
                    .executor_manager
                    .get_executor(&pn.lang)?
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

                cache.save_inline_result(pn.hash.clone(), &final_result)?;

                Ok(ExecutionOutput {
                    result: ExecutionResult::Text(final_result),
                    warnings: vec![],
                    error: None,
                })
            }
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
// Per-node output helpers (called from execute_pass)
// ---------------------------------------------------------------------------

/// Format the output of a freshly executed node.
fn format_executed_node(
    pn: &PlannedNode<'_>,
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
fn inert_output(pn: &PlannedNode<'_>, backend: &TypstBackend, config: &Config) -> String {
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
    pn: &PlannedNode<'_>,
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
fn format_error_block_for_node(node: &ExecutableNode<'_>, lang: &str, error_msg: &str) -> String {
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
fn build_executable_nodes(doc: &Document) -> Vec<ExecutableNode<'_>> {
    let mut nodes: Vec<ExecutableNode<'_>> = doc
        .chunks
        .iter()
        .map(ExecutableNode::Chunk)
        .chain(doc.inline_exprs.iter().map(ExecutableNode::InlineExpr))
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
fn register_freeze_objects(
    chunk: &Chunk,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    project_root: &Path,
) -> Result<()> {
    let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
    let exec = executor_manager.get_executor(&chunk.language)?;
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
        cache.metadata.freeze_objects.insert(
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
/// Errors immediately if any freeze object's current hash differs from its registered hash.
fn check_freeze_contract(
    pn: &PlannedNode<'_>,
    executor_manager: &mut ExecutorManager,
    cache: &Cache,
) -> Result<()> {
    let freeze_entries: Vec<_> = cache
        .metadata
        .freeze_objects
        .values()
        .filter(|info| info.language == pn.lang)
        .collect();

    if freeze_entries.is_empty() {
        return Ok(());
    }

    let exec = executor_manager.get_executor(&pn.lang)?;
    let chunk_name = match &pn.node {
        ExecutableNode::Chunk(c) => c.name.as_deref().unwrap_or("unnamed"),
        ExecutableNode::InlineExpr(_) => "inline",
    };

    for info in freeze_entries {
        let current_hash = exec
            .hash_object(&info.name)
            .context(format!("Failed to hash freeze object '{}'", info.name))?;

        if current_hash != info.hash {
            anyhow::bail!(
                "❌ Freeze contract violated!\n\n\
                 Object '{}' ({}) was declared as frozen in chunk '{}' but was modified in chunk '{}'.\n\n\
                 Expected hash: {}\n\
                 Current hash:  {}\n\n\
                 Frozen objects must not be mutated after declaration.\n\
                 Output file NOT generated to preserve reproducibility.",
                info.name,
                info.language,
                info.created_in_chunk,
                chunk_name,
                info.hash,
                current_hash
            );
        }
    }

    Ok(())
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
