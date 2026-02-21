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

// From section 3.1 and 6.1 (Semaine 2) of the reference document

pub struct Compiler {
    executor_manager: ExecutorManager,
    config: Config,
    project_root: PathBuf,
    cache_dir: PathBuf,
}

impl Compiler {
    /// Create a new compiler, searching for knot.toml starting from the given file path
    ///
    /// # Arguments
    /// * `knot_file_path` - Path to the .knot file being compiled (used to find project root)
    pub fn new(knot_file_path: &Path) -> Result<Self> {
        // Find project root by searching for knot.toml in parent directories
        // find_project_root() handles both files and directories automatically
        let project_root = Config::find_project_root(knot_file_path)?;

        // Load config from the project root
        let config_path = project_root.join("knot.toml");
        let config = if config_path.exists() {
            Config::load_from_path(&config_path)?
        } else {
            Config::default()
        };

        // Determine isolated cache directory for this file
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

    /// Compiles a document by executing its code chunks and generating a new Typst source file.
    ///
    /// `source_file` is the filename of the `.knot` source (e.g. `"chapter1.knot"`).
    /// It is embedded in `// #KNOT-SYNC` comments so the CLI and editors can map
    /// positions in the compiled `main.typ` back to the original `.knot` file.
    pub fn compile(&mut self, doc: &Document, source_file: &str) -> Result<String> {
        let mut cache = Cache::new(self.cache_dir.clone())?;
        let backend = crate::backend::TypstBackend::new();

        // Tracks the hash of the last chunk for EACH language (for chaining)
        let mut last_hash_per_lang: HashMap<String, String> = HashMap::new();

        // Manages snapshot loading/saving
        let mut snapshot_manager = SnapshotManager::new();

        let mut typst_output = String::new();
        let mut last_pos = 0;

        // Track constant objects: name -> (hash, chunk_name)
        let mut constant_objects: HashMap<String, (String, String)> = HashMap::new();

        // Phase 1: Build a sorted list of all executable nodes (chunks and inline expressions)
        let mut executable_nodes: Vec<ExecutableNode> = Vec::new();
        for chunk in &doc.chunks {
            executable_nodes.push(ExecutableNode::Chunk(chunk));
        }
        for inline_expr in &doc.inline_exprs {
            executable_nodes.push(ExecutableNode::InlineExpr(inline_expr));
        }
        executable_nodes.sort_by_key(|node| match node {
            ExecutableNode::Chunk(chunk) => chunk.start_byte,
            ExecutableNode::InlineExpr(inline_expr) => inline_expr.start,
        });

        info!(
            "🔧 Processing {} executable nodes...",
            executable_nodes.len()
        );

        let mut broken_languages = std::collections::HashSet::new();

        // Phase 2: Iterate through sorted nodes, execute, and build output
        for node in executable_nodes {
            let (node_start, node_end, lang) = match node {
                ExecutableNode::Chunk(chunk) => {
                    (chunk.start_byte, chunk.end_byte, chunk.language.as_str())
                }
                ExecutableNode::InlineExpr(inline_expr) => (
                    inline_expr.start,
                    inline_expr.end,
                    inline_expr.language.as_str(),
                ),
            };

            // Get previous hash for this language (or empty string if first chunk)
            let previous_hash = last_hash_per_lang.get(lang).cloned().unwrap_or_default();

            // Append raw text before the current node
            if node_start > last_pos {
                typst_output.push_str(&doc.source[last_pos..node_start]);
            }

            if broken_languages.contains(lang) {
                // This language is broken: render as inert.
                // Chunks still receive KNOT-SYNC markers so sync mapping stays correct.
                let result_str = match node {
                    ExecutableNode::Chunk(chunk) => {
                        let (res, _) = chunk_processor::process_chunk(
                            chunk,
                            &mut self.executor_manager,
                            &mut cache,
                            &previous_hash,
                            &self.config,
                            true, // is_inert
                            &backend,
                        )?;
                        res
                    }
                    ExecutableNode::InlineExpr(inline_expr) => {
                        format!(
                            "#text(fill: luma(150))[`{{{} {}}}`]",
                            inline_expr.language, inline_expr.code
                        )
                    }
                };
                if let ExecutableNode::Chunk(chunk) = node {
                    typst_output.push_str(&format!(
                        "// #KNOT-SYNC source={} line={}\n",
                        source_file,
                        chunk.range.start.line + 1,
                    ));
                    typst_output.push_str(&result_str);
                    if !result_str.is_empty() && !result_str.ends_with('\n') {
                        typst_output.push('\n');
                    }
                    typst_output.push_str("// END-KNOT-SYNC\n");
                } else {
                    typst_output.push_str(&result_str);
                }
                last_pos = node_end;
                // chunk.end_byte stops just before the closing fence's trailing \n;
                // advance past it so the next verbatim slice doesn't gain an extra blank line.
                if matches!(node, ExecutableNode::Chunk(_))
                    && last_pos < doc.source.len()
                    && doc.source.as_bytes()[last_pos] == b'\n'
                {
                    last_pos += 1;
                }
                continue;
            }

            // --- PROACTIVE STATE RESTORATION ---
            snapshot_manager.restore_if_needed(
                lang,
                &previous_hash,
                &mut self.executor_manager,
                &cache,
                &self.project_root,
            )?;

            let execution_result = match node {
                ExecutableNode::Chunk(chunk) => chunk_processor::process_chunk(
                    chunk,
                    &mut self.executor_manager,
                    &mut cache,
                    &previous_hash,
                    &self.config,
                    false, // is_inert
                    &backend,
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
                    // Fatal execution error: insert red error block and mark language as broken.
                    // The block is wrapped with KNOT-SYNC markers like any other chunk output.
                    let error_msg = format!("{}", e).replace('"', "\\\"");
                    let error_block = format!(
                        "#code-chunk(
    lang: \"{}\",
    is-inert: false,
    errors: ([#local(zebra-fill: none)[\n=== Execution Error ({})\nIn {} `{}`\n\n```\n{}\n```\n\n_Execution of subsequent `{}` blocks has been suspended._]],)
)\n",
                        lang,
                        lang,
                        match node {
                            ExecutableNode::Chunk(_) => "chunk",
                            ExecutableNode::InlineExpr(_) => "inline expression",
                        },
                        match node {
                            ExecutableNode::Chunk(c) => c.name.as_deref().unwrap_or("unnamed"),
                            ExecutableNode::InlineExpr(_) => "inline",
                        },
                        error_msg,
                        lang
                    );
                    if let ExecutableNode::Chunk(chunk) = node {
                        typst_output.push_str(&format!(
                            "// #KNOT-SYNC source={} line={}\n",
                            source_file,
                            chunk.range.start.line + 1,
                        ));
                        typst_output.push_str(&error_block);
                        typst_output.push_str("// END-KNOT-SYNC\n");
                    } else {
                        typst_output.push_str(&error_block);
                    }
                    last_pos = node_end;
                    if matches!(node, ExecutableNode::Chunk(_))
                        && last_pos < doc.source.len()
                        && doc.source.as_bytes()[last_pos] == b'\n'
                    {
                        last_pos += 1;
                    }
                    broken_languages.insert(lang.to_string());
                    continue;
                }
            };

            // Handle constant objects declared in this chunk (only if execution succeeded)
            if let ExecutableNode::Chunk(chunk) = node
                && !chunk.options.constant.is_empty()
            {
                let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
                let exec = self.executor_manager.get_executor(&chunk.language)?;
                let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);

                for obj_name in &chunk.options.constant {
                    // 1. Hash the object
                    let obj_hash = exec
                        .hash_object(obj_name)
                        .context(format!("Failed to hash constant object '{}'", obj_name))?;

                    // 2. Save to content-addressed storage
                    exec.save_constant(obj_name, &obj_hash, &cache_dir)
                        .context(format!("Failed to save constant object '{}'", obj_name))?;

                    // 3. Get file size for metadata
                    let ext = exec.object_extension();
                    let object_path = cache_dir
                        .join("objects")
                        .join(format!("{}.{}", obj_hash, ext));
                    let size_bytes = std::fs::metadata(&object_path)?.len();

                    // 4. Track for later verification
                    constant_objects
                        .insert(obj_name.to_string(), (obj_hash.clone(), chunk_name.clone()));

                    // 5. Add to cache metadata
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
            }

            // Update hash chain for this language
            last_hash_per_lang.insert(lang.to_string(), node_hash.clone());

            // After execution (or cache hit), we update the snapshot state
            snapshot_manager.update_after_node(
                lang,
                &node_hash,
                &previous_hash,
                &mut self.executor_manager,
                &cache,
                &self.project_root,
            )?;

            // Wrap the compiled chunk with sync markers so the CLI and editors can
            // map any position in the compiled .typ back to the originating .knot file.
            if let ExecutableNode::Chunk(chunk) = node {
                typst_output.push_str(&format!(
                    "// #KNOT-SYNC source={} line={}\n",
                    source_file,
                    chunk.range.start.line + 1,
                ));
                typst_output.push_str(&result_str);
                // Ensure END-KNOT-SYNC is always on its own line.
                if !result_str.is_empty() && !result_str.ends_with('\n') {
                    typst_output.push('\n');
                }
                typst_output.push_str("// END-KNOT-SYNC\n");
            } else {
                typst_output.push_str(&result_str);
            }

            last_pos = node_end;
            // chunk.end_byte stops just before the closing fence's trailing \n;
            // advance past it so the next verbatim slice doesn't gain an extra blank line.
            if matches!(node, ExecutableNode::Chunk(_))
                && last_pos < doc.source.len()
                && doc.source.as_bytes()[last_pos] == b'\n'
            {
                last_pos += 1;
            }
        }

        // Append any remaining raw text after the last node
        if last_pos < doc.source.len() {
            typst_output.push_str(&doc.source[last_pos..]);
        }

        info!("✓ All nodes processed.");

        // Final verification: Check that constant objects were not modified
        if !constant_objects.is_empty() {
            info!(
                "🔍 Verifying {} constant objects...",
                constant_objects.len()
            );

            for (obj_name, (initial_hash, chunk_name)) in &constant_objects {
                let info = cache.metadata.constant_objects.get(obj_name).unwrap();
                let exec = self.executor_manager.get_executor(&info.language)?;
                let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);

                // Try to load the constant object just for verification (idempotent)
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
        }

        // Save metadata (includes constant_objects info)
        cache.save_metadata()?;

        Ok(typst_output)
    }
}
