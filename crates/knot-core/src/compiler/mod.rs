use crate::executors::r::RExecutor;
use crate::executors::{ConstantObjectHandler, LanguageExecutor};
use crate::parser::{Chunk, Document, InlineExpr};
use crate::config::Config;
use crate::cache::ConstantObjectInfo;
use std::collections::HashMap;
use std::path::Path;

pub mod chunk_processor;
pub mod inline_processor;

/// Represents a node in the document that can be executed.
pub enum ExecutableNode<'a> {
    Chunk(&'a Chunk),
    InlineExpr(&'a InlineExpr),
}

use crate::cache::Cache;
use crate::get_cache_dir;
use anyhow::{Context, Result};
use log::info;
use std::path::PathBuf;

// From section 3.1 and 6.1 (Semaine 2) of the reference document

pub struct Compiler {
    r_executor: Option<RExecutor>,
    config: Config,
    cache_dir: PathBuf,
}

impl Compiler {
    /// Create a new compiler, searching for knot.toml starting from the given file path
    ///
    /// # Arguments
    /// * `knot_file_path` - Path to the .knot file being compiled (used to find project root)
    pub fn new(knot_file_path: &Path) -> Result<Self> {
        // Find project root by searching for knot.toml in parent directories
        let start_dir = knot_file_path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| knot_file_path.parent().unwrap_or(Path::new(".")).to_path_buf());

        let (config, project_root) = Config::find_and_load(&start_dir)?;
        let r_helper_path = config.r_helper_path(&project_root);

        if let Some(ref path) = r_helper_path {
            info!("Using R helper from knot.toml: {}", path.display());
        }

        // Determine isolated cache directory for this file
        let file_stem = knot_file_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");
        let cache_dir = get_cache_dir(&project_root, file_stem);
        
        info!("📦 Cache directory: {}", cache_dir.display());

        let r_executor = RExecutor::new(cache_dir.clone(), r_helper_path)
            .context("Failed to initialize R executor")?;

        Ok(Self {
            r_executor: Some(r_executor),
            config,
            cache_dir,
        })
    }

    /// Compiles a document by executing its code chunks and generating a new Typst source file.
    pub fn compile(&mut self, doc: &Document) -> Result<String> {
        let mut cache = Cache::new(self.cache_dir.clone())?;
        let mut previous_hash = String::new();
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

        // Initialize R executor
        if let Some(ref mut r_exec) = self.r_executor {
            r_exec.initialize()?;
        }

        info!("🔧 Processing {} executable nodes...", executable_nodes.len());

        // Phase 2: Iterate through sorted nodes, execute, and build output
        for node in executable_nodes {
            let (node_start, node_end) = match node {
                ExecutableNode::Chunk(chunk) => (chunk.start_byte, chunk.end_byte),
                ExecutableNode::InlineExpr(inline_expr) => (inline_expr.start, inline_expr.end),
            };

            // Append raw text before the current node
            if node_start > last_pos {
                typst_output.push_str(&doc.source[last_pos..node_start]);
            }

            let (result_str, node_hash) = match node {
                ExecutableNode::Chunk(chunk) => {
                    let result = chunk_processor::process_chunk(chunk, &mut self.r_executor, &mut cache, &previous_hash, &self.config.defaults)?;

                    // Handle constant objects declared in this chunk
                    if !chunk.options.constant.is_empty() {
                        let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();

                        if let Some(ref mut r_exec) = self.r_executor {
                            let cache_dir = get_cache_dir();

                            for obj_name in &chunk.options.constant {
                                // 1. Hash the object
                                let obj_hash = r_exec.hash_object(obj_name)
                                    .context(format!("Failed to hash constant object '{}'", obj_name))?;

                                // 2. Save to content-addressed storage
                                r_exec.save_constant(obj_name, &obj_hash, &cache_dir)
                                    .context(format!("Failed to save constant object '{}'", obj_name))?;

                                // 3. Get file size for metadata
                                let object_path = cache_dir.join("objects").join(format!("{}.rds", obj_hash));
                                let size_bytes = std::fs::metadata(&object_path)?.len();

                                // 4. Track for later verification
                                constant_objects.insert(obj_name.clone(), (obj_hash.clone(), chunk_name.clone()));

                                // 5. Add to cache metadata
                                cache.metadata.constant_objects.insert(
                                    obj_name.clone(),
                                    ConstantObjectInfo {
                                        hash: obj_hash,
                                        size_bytes,
                                        language: "r".to_string(),
                                        created_in_chunk: chunk_name.clone(),
                                        created_at: chrono::Utc::now().to_rfc3339(),
                                    }
                                );

                                log::info!("🔒 Constant object '{}' declared in chunk '{}'", obj_name, chunk_name);
                            }
                        }
                    }

                    result
                }
                ExecutableNode::InlineExpr(inline_expr) => {
                    inline_processor::process_inline_expr(inline_expr, &mut self.r_executor, &mut cache, &previous_hash)?
                }
            };

            // After execution/cache: Ensure snapshot exists
            let snapshot_path = cache.get_snapshot_path(&node_hash);
            let snapshot_exists = cache.has_snapshot(&node_hash);

            if let Some(ref mut r_exec) = self.r_executor {
                let cache_dir = get_cache_dir();

                if !snapshot_exists {
                    // Node was executed (not from cache), save the snapshot.
                    // Exclude constant objects from snapshot to keep it lightweight
                    for obj_name in constant_objects.keys() {
                        r_exec.remove_from_env(obj_name)
                            .context(format!("Failed to remove constant object '{}' from environment", obj_name))?;
                    }

                    r_exec.save_session(&snapshot_path)
                        .context(format!("Failed to save session snapshot for hash {}", &node_hash[..8]))?;

                    // Restore constant objects to environment
                    for (obj_name, (obj_hash, _)) in &constant_objects {
                        r_exec.load_constant(obj_name, obj_hash, &cache_dir)
                            .context(format!("Failed to restore constant object '{}' to environment", obj_name))?;
                    }

                    log::debug!("💾 Saved lightweight snapshot for node {} (excluded {} constant objects)",
                               &node_hash[..8], constant_objects.len());
                } else {
                    // Node was cached (skipped execution).
                    // Load the snapshot, then restore constant objects
                    log::debug!("📦 Using existing snapshot for node {}", &node_hash[..8]);
                    r_exec.load_session(&snapshot_path)
                        .context(format!("Failed to load session snapshot for hash {}", &node_hash[..8]))?;

                    // Restore constant objects from content-addressed storage
                    for (obj_name, (obj_hash, _)) in &constant_objects {
                        r_exec.load_constant(obj_name, obj_hash, &cache_dir)
                            .context(format!("Failed to load constant object '{}' from cache", obj_name))?;
                    }

                    log::debug!("📂 Loaded snapshot for node {} (+ {} constant objects)",
                               &node_hash[..8], constant_objects.len());
                }
            } else if snapshot_exists {
                log::debug!("📦 Using existing snapshot for node {} (no R executor)", &node_hash[..8]);
            }

            typst_output.push_str(&result_str);
            previous_hash = node_hash;
            last_pos = node_end;
        }

        // Append any remaining raw text after the last node
        if last_pos < doc.source.len() {
            typst_output.push_str(&doc.source[last_pos..]);
        }

        info!("✓ All nodes processed.");

        // Final verification: Check that constant objects were not modified
        if !constant_objects.is_empty() {
            info!("🔍 Verifying {} constant objects...", constant_objects.len());

            if let Some(ref mut r_exec) = self.r_executor {
                for (obj_name, (initial_hash, chunk_name)) in &constant_objects {
                    let final_hash = r_exec.hash_object(obj_name)
                        .context(format!("Failed to verify constant object '{}'", obj_name))?;

                    if &final_hash != initial_hash {
                        anyhow::bail!(
                            "❌ Constant object verification failed!\n\n\
                             Object '{}' was declared as constant in chunk '{}' but was modified during execution.\n\n\
                             Initial hash: {}\n\
                             Final hash:   {}\n\n\
                             This violates the constant object contract. The object must remain immutable after creation.\n\
                             Output file NOT generated to preserve reproducibility.",
                            obj_name, chunk_name, initial_hash, final_hash
                        );
                    }
                }

                info!("✓ All constant objects verified successfully.");
            }
        }

        // Save metadata (includes constant_objects info)
        cache.save_metadata()?;

        Ok(typst_output)
    }
}
