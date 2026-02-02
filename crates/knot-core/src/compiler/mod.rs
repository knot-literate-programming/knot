use crate::executors::r::RExecutor;
use crate::executors::LanguageExecutor;
use crate::parser::{Chunk, Document, InlineExpr};
use crate::config::Config;
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

// From section 3.1 and 6.1 (Semaine 2) of the reference document

pub struct Compiler {
    r_executor: Option<RExecutor>,
    config: Config,
    // In the future, we'll have more executors
    // lilypond_executor: Option<LilypondExecutor>,
    // python_executor: Option<PythonExecutor>,
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

        let cache_dir = get_cache_dir();
        let r_executor = RExecutor::new(cache_dir, r_helper_path)
            .context("Failed to initialize R executor")?;

        Ok(Self {
            r_executor: Some(r_executor),
            config,
        })
    }

    /// Compiles a document by executing its code chunks and generating a new Typst source file.
    pub fn compile(&mut self, doc: &Document) -> Result<String> {
        let mut cache = Cache::new(get_cache_dir())?;
        let mut previous_hash = String::new();
        let mut typst_output = String::new();
        let mut last_pos = 0;

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
                    chunk_processor::process_chunk(chunk, &mut self.r_executor, &mut cache, &previous_hash, &self.config.defaults)?
                }
                ExecutableNode::InlineExpr(inline_expr) => {
                    inline_processor::process_inline_expr(inline_expr, &mut self.r_executor, &mut cache, &previous_hash)?
                }
            };

            typst_output.push_str(&result_str);
            previous_hash = node_hash;
            last_pos = node_end;
        }

        // Append any remaining raw text after the last node
        if last_pos < doc.source.len() {
            typst_output.push_str(&doc.source[last_pos..]);
        }

        info!("✓ All nodes processed.");
        
        Ok(typst_output)
    }
}
