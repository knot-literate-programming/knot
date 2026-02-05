use crate::executors::{ExecutorManager};
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
use crate::defaults::Defaults;
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

        let executor_manager = ExecutorManager::new(cache_dir.clone(), r_helper_path);

        Ok(Self {
            executor_manager,
            config,
            project_root,
            cache_dir,
        })
    }

    /// Compiles a document by executing its code chunks and generating a new Typst source file.
    pub fn compile(&mut self, doc: &Document) -> Result<String> {
        let mut cache = Cache::new(self.cache_dir.clone())?;
        
        // Tracks the hash of the last chunk for EACH language (for chaining)
        let mut last_hash_per_lang: HashMap<String, String> = HashMap::new();
        
        // Tracks the hash of the snapshot currently loaded in EACH executor (for restoration)
        let mut loaded_snapshot_per_lang: HashMap<String, String> = HashMap::new();
        
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

        info!("🔧 Processing {} executable nodes...", executable_nodes.len());

        // Phase 2: Iterate through sorted nodes, execute, and build output
        for node in executable_nodes {
            let (node_start, node_end, lang) = match node {
                ExecutableNode::Chunk(chunk) => (chunk.start_byte, chunk.end_byte, chunk.language.as_str()),
                ExecutableNode::InlineExpr(inline_expr) => (inline_expr.start, inline_expr.end, inline_expr.language.as_str()),
            };

            // Get previous hash for this language (or empty string if first chunk)
            let previous_hash = last_hash_per_lang.get(lang).cloned().unwrap_or_default();

            // Append raw text before the current node
            if node_start > last_pos {
                typst_output.push_str(&doc.source[last_pos..node_start]);
            }

            // --- PROACTIVE STATE RESTORATION ---
            // If we are going to execute code (not purely cache hit check yet, but preparation),
            // we must ensure the executor is in the state corresponding to `previous_hash`.
            // Note: process_chunk does the cache check. If it's a hit, we don't need to restore.
            // But process_chunk also calculates the current hash. We need to do this carefully.
            
            // Actually, we can defer restoration until inside process_chunk? No, because process_chunk
            // handles execution. Ideally, we should check cache here, restore if needed, then execute.
            // But compute_hash is inside process_chunk.
            
            // Strategy: We pass `previous_hash` to process_chunk.
            // Inside process_chunk (conceptually), we compute current_hash.
            // If cache miss, we execute.
            // BUT before executing, we must be sure state is correct.
            
            // To do this cleanly without moving all hash logic out of process_chunk,
            // we can trust that process_chunk will use the executor.
            // We should ensure executor is ready BEFORE calling process_chunk IF we suspect execution might happen.
            // But we don't know the current hash yet to check cache!
            
            // Let's modify process_chunk to return "NeedExecution(hash)" or "Cached(result, hash)".
            // This is too much refactoring.
            
            // Simpler approach:
            // We blindly trust that if the executor is not at `previous_hash`, we should try to load it.
            // BUT `previous_hash` snapshot might not exist if the previous chunk was just executed in memory
            // and we haven't saved it yet? No, we save snapshots after every execution.
            
            // Wait, if we just executed Chunk N, `previous_hash` for Chunk N+1 is Hash(N).
            // `loaded_snapshot` is also Hash(N) because we just finished N.
            // So for sequential execution, it matches.
            
            // If we skipped Chunk N (cache hit), `previous_hash` is Hash(N).
            // But `loaded_snapshot` might be Hash(N-1) (or older).
            // So we detect mismatch. We load Snapshot(N).
            
            if let Ok(exec) = self.executor_manager.get_executor(lang) {
                let current_loaded = loaded_snapshot_per_lang.get(lang).cloned().unwrap_or_default();
                
                // If the executor is not in the state of the previous chunk, try to restore it
                // Only if previous_hash is not empty (i.e., not the very first chunk)
                if !previous_hash.is_empty() && current_loaded != previous_hash {
                    let ext = exec.snapshot_extension();
                    let snapshot_path = cache.get_snapshot_path(&previous_hash, ext);
                    
                    if snapshot_path.exists() {
                        log::debug!("Restoring state for {} from {}", lang, &previous_hash[..8]);
                        // We also need to restore constants
                        let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);
                        
                        if let Err(e) = exec.load_session(&snapshot_path) {
                             log::warn!("Failed to restore session: {}", e);
                        } else {
                             // Also restore constants
                             for (obj_name, info) in &cache.metadata.constant_objects {
                                if info.language == lang {
                                    if let Err(e) = exec.load_constant(obj_name, &info.hash, &cache_dir) {
                                        log::warn!("Failed to load constant {}: {}", obj_name, e);
                                    }
                                }
                            }
                            loaded_snapshot_per_lang.insert(lang.to_string(), previous_hash.clone());
                        }
                    }
                }
            }

            let (result_str, node_hash) = match node {
                ExecutableNode::Chunk(chunk) => {
                    let result = chunk_processor::process_chunk(
                        chunk, 
                        &mut self.executor_manager, 
                        &mut cache, 
                        &previous_hash, 
                        &self.config.defaults
                    )?;

                    // Handle constant objects declared in this chunk
                    if !chunk.options.constant.is_empty() {
                        let chunk_name = chunk.name.as_deref().unwrap_or("unnamed").to_string();
                        let exec = self.executor_manager.get_executor(&chunk.language)?;
                        let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);

                        for obj_name in &chunk.options.constant {
                            // 1. Hash the object
                            let obj_hash = exec.hash_object(obj_name)
                                .context(format!("Failed to hash constant object '{}'", obj_name))?;

                            // 2. Save to content-addressed storage
                            exec.save_constant(obj_name, &obj_hash, &cache_dir)
                                .context(format!("Failed to save constant object '{}'", obj_name))?;

                            // 3. Get file size for metadata
                            let ext = exec.object_extension();
                            let object_path = cache_dir.join("objects").join(format!("{}.{}", obj_hash, ext));
                            let size_bytes = std::fs::metadata(&object_path)?.len();

                            // 4. Track for later verification
                            constant_objects.insert(obj_name.clone(), (obj_hash.clone(), chunk_name.clone()));

                            // 5. Add to cache metadata
                            cache.metadata.constant_objects.insert(
                                obj_name.clone(),
                                ConstantObjectInfo {
                                    hash: obj_hash,
                                    size_bytes,
                                    language: chunk.language.clone(),
                                    created_in_chunk: chunk_name.clone(),
                                    created_at: chrono::Utc::now().to_rfc3339(),
                                }
                            );

                            log::info!("🔒 Constant object '{}' ({}) declared in chunk '{}'", obj_name, chunk.language, chunk_name);
                        }
                    }

                    result
                }
                ExecutableNode::InlineExpr(inline_expr) => {
                    inline_processor::process_inline_expr(
                        inline_expr, 
                        &mut self.executor_manager, 
                        &mut cache, 
                        &previous_hash
                    )?
                }
            };

            // Update hash chain for this language
            last_hash_per_lang.insert(lang.to_string(), node_hash.clone());

            // After execution (or cache hit), we update the "loaded snapshot" state
            // If it was a cache HIT, we didn't execute, so state is technically "previous_hash".
            // BUT if we want to be ready for the NEXT chunk, we conceptually are at "node_hash".
            // However, the executor is physically at "previous_hash" if we didn't run anything.
            // If we DID run (cache miss), the executor is physically at "node_hash" (because we ran the code).
            
            // To unify: We always save the snapshot for "node_hash" if it doesn't exist.
            // And we consider the executor to be at "node_hash".
            
            if let Ok(exec) = self.executor_manager.get_executor(lang) {
                let ext = exec.snapshot_extension();
                let snapshot_path = cache.get_snapshot_path(&node_hash, ext);
                let snapshot_exists = cache.has_snapshot(&node_hash, ext);
                let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);

                if !snapshot_exists {
                    // CASE 1: Cache Miss (Executed)
                    // The executor has just run the code. It is in state `node_hash`.
                    
                    // Save the new snapshot
                    for (obj_name, info) in &cache.metadata.constant_objects {
                        if info.language == lang {
                            exec.remove_from_env(obj_name)
                                .context(format!("Failed to remove constant object '{}' from environment", obj_name))?;
                        }
                    }

                    exec.save_session(&snapshot_path)
                        .context(format!("Failed to save {} session snapshot for hash {}", lang, &node_hash[..8]))?;

                    // Restore constant objects to environment
                    for (obj_name, info) in &cache.metadata.constant_objects {
                        if info.language == lang {
                            exec.load_constant(obj_name, &info.hash, &cache_dir)
                                .context(format!("Failed to restore constant object '{}' to environment", obj_name))?;
                        }
                    }

                    log::debug!("💾 Saved lightweight snapshot for node {} (lang: {})", &node_hash[..8], lang);
                    
                    // Update tracked state
                    loaded_snapshot_per_lang.insert(lang.to_string(), node_hash.clone());
                    
                } else {
                    // CASE 2: Cache Hit (Skipped)
                    // The executor did NOT run the code. It is still in state `previous_hash`.
                    // We do NOT load the new snapshot `node_hash` immediately.
                    // Why? Because if the next chunk is also a cache hit, we avoid loading intermediate snapshots!
                    // We only load when needed (at the start of the loop, if we detect a mismatch).
                    
                    // So we do NOTHING here. `loaded_snapshot_per_lang` remains `previous_hash`.
                    log::debug!("⚡ Chunk {} cached. Executor stays at state {}.", &node_hash[..8], &previous_hash[..8]);
                }
            }

            typst_output.push_str(&result_str);
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

            for (obj_name, (initial_hash, chunk_name)) in &constant_objects {
                let info = cache.metadata.constant_objects.get(obj_name).unwrap();
                // Ensure executor is loaded with the right state to verify?
                // Actually, for verification, we might need to be careful if we skipped chunks.
                // But constant objects are by definition constant, so their hash should be verifyable 
                // in ANY state where they exist.
                // If we skipped the chunk that created them, they might not be loaded!
                
                // So we need to ensure they are loaded.
                let exec = self.executor_manager.get_executor(&info.language)?;
                let cache_dir = self.project_root.join(Defaults::CACHE_DIR_NAME);
                
                // Try to load the constant object just for verification (idempotent)
                exec.load_constant(obj_name, &info.hash, &cache_dir)
                    .context(format!("Failed to load constant '{}' for verification", obj_name))?;
                
                let final_hash = exec.hash_object(obj_name)
                    .context(format!("Failed to verify constant object '{}'", obj_name))?;

                if &final_hash != initial_hash {
                    anyhow::bail!(
                        "❌ Constant object verification failed!\n\n\
                         Object '{}' ({}) was declared as constant in chunk '{}' but was modified during execution.\n\n\
                         Initial hash: {}\n\
                         Final hash:   {}\n\n\
                         This violates the constant object contract. The object must remain immutable after creation.\n\
                         Output file NOT generated to preserve reproducibility.",
                        obj_name, info.language, chunk_name, initial_hash, final_hash
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
