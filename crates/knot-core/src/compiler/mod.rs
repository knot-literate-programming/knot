use crate::cache::ConstantObjectInfo;
use crate::config::Config;
use crate::executors::ExecutorManager;
use crate::parser::ast::{Chunk, Document, InlineExpr};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub mod chunk_processor;
pub mod formatters;
pub mod inline_processor;
pub mod sync;

pub use chunk_processor::{ChunkContext, ChunkExecutionState};

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
pub mod snapshot_manager;

/// Represents a node in the document that can be executed.
pub enum ExecutableNode<'a> {
    Chunk(&'a Chunk),
    InlineExpr(&'a InlineExpr),
}

use crate::cache::Cache;
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
        let mut cache = Cache::new(self.cache_dir.clone())?;
        let backend = crate::backend::TypstBackend::new();
        let mut last_hash_per_lang: HashMap<String, String> = HashMap::new();
        let mut snapshot_manager = SnapshotManager::new();
        let mut typst_output = String::new();
        let mut last_pos = 0;
        let mut constant_objects: HashMap<String, (String, String)> = HashMap::new();
        let mut broken_languages = std::collections::HashSet::new();

        let executable_nodes = build_executable_nodes(doc);
        info!(
            "🔧 Processing {} executable nodes...",
            executable_nodes.len()
        );

        for node in &executable_nodes {
            let (node_start, node_end, lang) = node_bounds(node);
            let previous_hash = last_hash_per_lang.get(lang).cloned().unwrap_or_default();

            if node_start > last_pos {
                typst_output.push_str(&doc.source[last_pos..node_start]);
            }

            // --- Normal execution path ---
            let ctx = ChunkContext {
                previous_hash: &previous_hash,
                config: &self.config,
                state: ChunkExecutionState::Ready,
                backend: &backend,
                project_root: &self.project_root,
            };

            // --- Inert path (language previously broken) ---
            if broken_languages.contains(lang) {
                let inert_ctx = ChunkContext {
                    state: ChunkExecutionState::Inert,
                    ..ctx
                };
                let result = render_inert_node(
                    node,
                    &mut self.executor_manager,
                    &mut cache,
                    &mut snapshot_manager,
                    &inert_ctx,
                )?;
                append_node_output(node, &result, source_file, &mut typst_output);
                last_pos = advance_last_pos(node, &doc.source, node_end);
                continue;
            }

            let execution_result = match node {
                ExecutableNode::Chunk(chunk) => chunk_processor::process_chunk(
                    chunk,
                    &mut self.executor_manager,
                    &mut cache,
                    &mut snapshot_manager,
                    &ctx,
                ),
                ExecutableNode::InlineExpr(inline_expr) => inline_processor::process_inline_expr(
                    inline_expr,
                    &mut self.executor_manager,
                    &mut cache,
                    &previous_hash,
                ),
            };

            let (result_str, node_hash) = match execution_result {
                Ok(res) => res,
                Err(e) => {
                    let error_block = format_error_block(node, lang, &e.to_string());
                    append_node_output(node, &error_block, source_file, &mut typst_output);
                    last_pos = advance_last_pos(node, &doc.source, node_end);
                    broken_languages.insert(lang.to_string());
                    continue;
                }
            };

            // Register constant objects declared by this chunk
            if let ExecutableNode::Chunk(chunk) = node
                && !chunk.options.constant.is_empty()
            {
                register_constant_objects(
                    chunk,
                    &mut self.executor_manager,
                    &mut cache,
                    &self.project_root,
                    &mut constant_objects,
                )?;
            }

            last_hash_per_lang.insert(lang.to_string(), node_hash.clone());
            snapshot_manager.update_after_node(
                lang,
                &node_hash,
                &previous_hash,
                &mut self.executor_manager,
                &cache,
                &self.project_root,
            )?;

            append_node_output(node, &result_str, source_file, &mut typst_output);
            last_pos = advance_last_pos(node, &doc.source, node_end);
        }

        if last_pos < doc.source.len() {
            typst_output.push_str(&doc.source[last_pos..]);
        }
        info!("✓ All nodes processed.");

        verify_constant_objects(
            &constant_objects,
            &mut self.executor_manager,
            &cache,
            &self.project_root,
        )?;
        cache.save_metadata()?;

        Ok(typst_output)
    }
}

// ---------------------------------------------------------------------------
// Private helpers for compile()
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

/// Returns `(start_byte, end_byte, language)` for a node.
fn node_bounds<'a>(node: &'a ExecutableNode<'a>) -> (usize, usize, &'a str) {
    match node {
        ExecutableNode::Chunk(c) => (c.start_byte, c.end_byte, c.language.as_str()),
        ExecutableNode::InlineExpr(e) => (e.start, e.end, e.language.as_str()),
    }
}

/// Advances `node_end` past a trailing `\n` for chunk nodes.
///
/// `chunk.end_byte` stops just before the closing fence's trailing newline.
/// Skipping it prevents an extra blank line in the output.
fn advance_last_pos(node: &ExecutableNode<'_>, source: &str, node_end: usize) -> usize {
    if matches!(node, ExecutableNode::Chunk(_))
        && node_end < source.len()
        && source.as_bytes()[node_end] == b'\n'
    {
        node_end + 1
    } else {
        node_end
    }
}

/// Appends `content` to `output`, wrapped with KNOT-SYNC markers for chunks.
/// Inline expressions are appended verbatim (no markers).
fn append_node_output(
    node: &ExecutableNode<'_>,
    content: &str,
    source_file: &str,
    output: &mut String,
) {
    if let ExecutableNode::Chunk(chunk) = node {
        output.push_str(&format!(
            "// #KNOT-SYNC source={} line={}\n",
            source_file,
            chunk.range.start.line + 1,
        ));
        output.push_str(content);
        if !content.is_empty() && !content.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("// END-KNOT-SYNC\n");
    } else {
        output.push_str(content);
    }
}

/// Renders a node for a language that previously errored (inert / greyed-out mode).
fn render_inert_node(
    node: &ExecutableNode<'_>,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    snapshot_manager: &mut SnapshotManager,
    ctx: &ChunkContext<'_>,
) -> Result<String> {
    match node {
        ExecutableNode::Chunk(chunk) => {
            let (res, _) = chunk_processor::process_chunk(
                chunk,
                executor_manager,
                cache,
                snapshot_manager,
                ctx,
            )?;
            Ok(res)
        }
        ExecutableNode::InlineExpr(inline_expr) => Ok(format!(
            "#text(fill: luma(150))[`{{{} {}}}`]",
            inline_expr.language, inline_expr.code
        )),
    }
}

/// Formats the Typst error block shown when a node fails to execute.
fn format_error_block(node: &ExecutableNode<'_>, lang: &str, error_msg: &str) -> String {
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

/// Saves all constant objects declared by a chunk and records them in the tracking map.
fn register_constant_objects(
    chunk: &Chunk,
    executor_manager: &mut ExecutorManager,
    cache: &mut Cache,
    project_root: &Path,
    constant_objects: &mut HashMap<String, (String, String)>,
) -> Result<()> {
    let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
    let exec = executor_manager.get_executor(&chunk.language)?;
    let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

    for obj_name in &chunk.options.constant {
        let obj_hash = exec
            .hash_object(obj_name)
            .context(format!("Failed to hash constant object '{}'", obj_name))?;

        exec.save_constant(obj_name, &obj_hash, &cache_dir)
            .context(format!("Failed to save constant object '{}'", obj_name))?;

        let ext = exec.object_extension();
        let object_path = cache_dir
            .join("objects")
            .join(format!("{}.{}", obj_hash, ext));
        let size_bytes = std::fs::metadata(&object_path)?.len();

        constant_objects.insert(obj_name.to_string(), (obj_hash.clone(), chunk_name.clone()));

        cache.metadata.constant_objects.insert(
            obj_name.to_string(),
            ConstantObjectInfo {
                hash: obj_hash,
                size_bytes,
                language: chunk.language.clone(),
                created_in_chunk: chunk_name.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );

        log::info!(
            "🔒 Constant object '{}' ({}) declared in chunk '{}'",
            obj_name,
            chunk.language,
            chunk_name
        );
    }
    Ok(())
}

/// Verifies that no constant object was mutated after its initial declaration.
fn verify_constant_objects(
    constant_objects: &HashMap<String, (String, String)>,
    executor_manager: &mut ExecutorManager,
    cache: &Cache,
    project_root: &Path,
) -> Result<()> {
    if constant_objects.is_empty() {
        return Ok(());
    }

    info!(
        "🔍 Verifying {} constant objects...",
        constant_objects.len()
    );

    for (obj_name, (initial_hash, chunk_name)) in constant_objects {
        let info = cache.metadata.constant_objects.get(obj_name).unwrap();
        let exec = executor_manager.get_executor(&info.language)?;
        let cache_dir = project_root.join(Defaults::CACHE_DIR_NAME);

        exec.load_constant(obj_name, &info.hash, &cache_dir)
            .context(format!(
                "Failed to load constant '{}' for verification",
                obj_name
            ))?;

        let final_hash = exec
            .hash_object(obj_name)
            .context(format!("Failed to verify constant object '{}'", obj_name))?;

        if &final_hash != initial_hash {
            anyhow::bail!(
                "❌ Constant object verification failed!\n\n\
                 Object '{}' ({}) was declared as constant in chunk '{}' but was modified during execution.\n\n\
                 Initial hash: {}\n\n\
                 Final hash:   {}\n\n\
                 This violates the constant object contract. The object must remain immutable after creation.\n\
                 Output file NOT generated to preserve reproducibility.",
                obj_name,
                info.language,
                chunk_name,
                initial_hash,
                final_hash
            );
        }
    }

    info!("✓ All constant objects verified successfully.");
    Ok(())
}
