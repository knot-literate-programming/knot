use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

// NOTE : Ces structures sont basées sur la section 3.5 du document de référence.
// La section 11.4 mentionne que les positions sont cruciales pour un futur LSP.
// Pour la phase 1, les positions exactes sont moins critiques, mais les structures
// sont là pour l'avenir.

/// Position dans le fichier (ligne/colonne, base 0)
/// Essentiel pour le support LSP (Language Server Protocol)
#[derive(Debug, Clone, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Plage dans le fichier, de `start` (inclusif) à `end` (exclusif)
#[derive(Debug, Clone, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ChunkOptions {
    // Boolean options: None means "use defaults"
    pub eval: Option<bool>,
    pub echo: Option<bool>,
    pub output: Option<bool>,
    pub cache: Option<bool>,

    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,

    // Graphics options (Phase 4)
    pub fig_width: Option<f64>,
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    pub fig_format: Option<String>,
    pub fig_alt: Option<String>,

    // Constant objects (Cache optimization)
    pub constant: Vec<String>,
}

impl ChunkOptions {
    /// Apply default values from knot.toml configuration
    ///
    /// Only applies defaults for fields that are None (not specified in chunk).
    /// Chunk-specific options always take priority over config defaults.
    ///
    /// Priority: chunk options > knot.toml defaults > hardcoded defaults
    pub fn apply_config_defaults(&mut self, defaults: &crate::config::ChunkDefaults) {
        // Boolean options: apply config defaults if not set in chunk
        if self.eval.is_none() {
            self.eval = defaults.eval;
        }
        if self.echo.is_none() {
            self.echo = defaults.echo;
        }
        if self.output.is_none() {
            self.output = defaults.output;
        }
        if self.cache.is_none() {
            self.cache = defaults.cache;
        }

        // Graphics options: apply config defaults if not set in chunk
        if self.fig_width.is_none() {
            self.fig_width = defaults.fig_width;
        }
        if self.fig_height.is_none() {
            self.fig_height = defaults.fig_height;
        }
        if self.dpi.is_none() {
            self.dpi = defaults.dpi;
        }
        if self.fig_format.is_none() {
            self.fig_format = defaults.fig_format.clone();
        }
    }

    /// Resolve all options to concrete values
    ///
    /// Applies hardcoded defaults for any options still None after config defaults.
    /// This is the final step that converts Option<bool> to bool.
    pub fn resolve(&self) -> ResolvedChunkOptions {
        ResolvedChunkOptions {
            eval: self.eval.unwrap_or(crate::defaults::Defaults::CHUNK_EVAL),
            echo: self.echo.unwrap_or(crate::defaults::Defaults::CHUNK_ECHO),
            output: self
                .output
                .unwrap_or(crate::defaults::Defaults::CHUNK_OUTPUT),
            cache: self.cache.unwrap_or(crate::defaults::Defaults::CHUNK_CACHE),

            label: self.label.clone(),
            caption: self.caption.clone(),
            depends: self.depends.clone(),

            fig_width: self
                .fig_width
                .unwrap_or(crate::defaults::Defaults::FIG_WIDTH),
            fig_height: self
                .fig_height
                .unwrap_or(crate::defaults::Defaults::FIG_HEIGHT),
            dpi: self.dpi.unwrap_or(crate::defaults::Defaults::DPI),
            fig_format: self
                .fig_format
                .clone()
                .unwrap_or_else(|| crate::defaults::Defaults::FIG_FORMAT.to_string()),
            fig_alt: self.fig_alt.clone(),

            constant: self.constant.clone(),
        }
    }
}

/// ChunkOptions with all values resolved to concrete types
///
/// This is what the compiler uses after applying chunk > config > hardcoded defaults.
#[derive(Debug, Clone)]
pub struct ResolvedChunkOptions {
    pub eval: bool,
    pub echo: bool,
    pub output: bool,
    pub cache: bool,

    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,

    pub fig_width: f64,
    pub fig_height: f64,
    pub dpi: u32,
    pub fig_format: String,
    pub fig_alt: Option<String>,

    pub constant: Vec<String>,
}

#[derive(Debug)]
pub struct Chunk {
    pub language: String,
    pub name: Option<String>,
    pub code: String,
    pub options: ChunkOptions,
    pub errors: Vec<String>,
    pub range: Range,      // Position du chunk entier (de ```{r}} à ```)
    pub code_range: Range, // Position du code seul à l'intérieur
    pub start_byte: usize,
    pub end_byte: usize,
    pub code_start_byte: usize,
    pub code_end_byte: usize,
}

/// Options for inline expressions
#[derive(Debug, Clone, PartialEq)]
pub struct InlineOptions {
    pub echo: bool,          // Show the inline code (default: false)
    pub eval: bool,          // Evaluate the code (default: true)
    pub output: bool,        // Show the result in the document (default: true)
    pub digits: Option<u32>, // Number of digits for numeric formatting
}

impl Default for InlineOptions {
    fn default() -> Self {
        Self {
            echo: crate::defaults::Defaults::INLINE_ECHO,
            eval: crate::defaults::Defaults::INLINE_EVAL,
            output: crate::defaults::Defaults::INLINE_OUTPUT,
            digits: None, // Use default formatting
        }
    }
}

/// Inline expression (e.g., `{r} nrow(df)` or `{r echo=false} x`)
#[derive(Debug, Clone)]
pub struct InlineExpr {
    pub language: String, // "r", "python", etc.
    pub code: String,     // The expression to evaluate
    pub start: usize,     // Byte offset in source
    pub end: usize,       // Byte offset in source (exclusive)
    pub code_start_byte: usize,
    pub code_end_byte: usize,
    pub options: InlineOptions,
    pub errors: Vec<String>,
}

pub struct Document {
    pub source: String,
    pub chunks: Vec<Chunk>,
    pub inline_exprs: Vec<InlineExpr>,
    pub errors: Vec<String>,
}

impl Document {
    // La logique de parsing utilise winnow (v2)
    pub fn parse(source: String) -> Result<Self> {
        let doc = super::winnow_parser::parse_document(&source);
        if !doc.errors.is_empty() {
            // For now, we still return Ok but the document contains errors.
            // This is good for LSP.
            // In the future, the compiler might want to check doc.errors.
        }
        Ok(doc)
    }
}
