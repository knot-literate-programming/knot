use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// Chunk Option Enums
// ============================================================================

/// What to display in the output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    /// Display both input code and output
    #[default]
    Both,
    /// Display only input code
    Input,
    /// Display only output
    Output,
}

/// How to layout input and output when both are displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Side-by-side layout
    #[default]
    Horizontal,
    /// Stacked layout
    Vertical,
}

/// Format for generated figures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FigFormat {
    /// SVG vector format
    #[default]
    Svg,
    /// PNG raster format
    Png,
}

impl FigFormat {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            FigFormat::Svg => "svg",
            FigFormat::Png => "png",
        }
    }
}

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

/// An error detected during chunk or inline expression parsing
#[derive(Debug, Clone, Serialize)]
pub struct ChunkError {
    pub message: String,
    /// Relative line offset within the chunk (0 = header line)
    pub line_offset: Option<usize>,
}

impl ChunkError {
    pub fn new(message: impl Into<String>, line_offset: Option<usize>) -> Self {
        Self {
            message: message.into(),
            line_offset,
        }
    }
}

// ============================================================================
// Chunk Options Macro System
// ============================================================================
//
// This macro system defines ChunkOptions with automatic generation of related
// types and methods. It uses markers to control behavior and configurability.
//
// ## Markers
//
// - [val]  : Required value with default, configurable in knot.toml
//            → ChunkOptions: Option<T>, ResolvedChunkOptions: T
//            → Included in ChunkDefaults for global configuration
//            → Example: eval: bool (knot.toml: eval = false)
//
// - [opt]  : Optional value without default, configurable in knot.toml
//            → ChunkOptions: Option<T>, ResolvedChunkOptions: Option<T>
//            → Included in ChunkDefaults for global configuration
//            → Example: gutter: String (knot.toml: gutter = "2em")
//
// - [meta] : Chunk-specific metadata, NOT configurable in knot.toml
//            → ChunkOptions: Option<T>, ResolvedChunkOptions: Option<T>
//            → NOT included in ChunkDefaults (chunk-specific only)
//            → Example: label: String (only in chunk header)
//
// - [col]  : Collection, NOT configurable in knot.toml
//            → ChunkOptions: Vec<T>, ResolvedChunkOptions: Vec<T>
//            → NOT included in ChunkDefaults (chunk-specific only)
//            → Example: depends: Vec<PathBuf>
//
// ## Auto-generated code
//
// The define_options! macro generates:
// - ChunkOptions struct (for parsing YAML chunk options)
// - ResolvedChunkOptions struct (with concrete types after resolution)
// - resolve() method (applies hardcoded defaults)
// - apply_config_defaults() method (applies knot.toml defaults)
//   → Only processes [val] and [opt] fields (skips [meta] and [col])
// - option_metadata() method (returns metadata for template generation)
//
// ============================================================================

/// Metadata for a single chunk option (used for template generation)
#[derive(Debug, Clone)]
pub struct OptionMetadata {
    /// Option kind: "val", "opt", "meta", or "col"
    pub kind: &'static str,
    /// Field name in Rust code
    pub name: &'static str,
    /// Type as string
    pub type_name: &'static str,
    /// Default value as string
    pub default_value: &'static str,
    /// Documentation string
    pub doc: &'static str,
}

impl OptionMetadata {
    /// Get the serde name (converts snake_case to kebab-case for options like fig_width → fig-width)
    pub fn serde_name(&self) -> String {
        self.name.replace('_', "-")
    }
}

/// Internal macros to handle type expansion and resolution
macro_rules! expand_type {
    (val, $type:ty) => { Option<$type> };
    (opt, $type:ty) => { Option<$type> };
    (meta, $type:ty) => { Option<$type> };
    (col, $type:ty) => { $type };
}

macro_rules! expand_resolved_type {
    (val, $type:ty) => { $type };
    (opt, $type:ty) => { Option<$type> };
    (meta, $type:ty) => { Option<$type> };
    (col, $type:ty) => { $type };
}

macro_rules! expand_resolve {
    ($val:expr, val, $type:ty, $default:expr) => {
        $val.clone().unwrap_or_else(|| $default)
    };
    ($val:expr, opt, $type:ty, $default:expr) => {
        $val.clone()
    };
    ($val:expr, meta, $type:ty, $default:expr) => {
        $val.clone()
    };
    ($val:expr, col, $type:ty, $default:expr) => {
        $val.clone()
    };
}

/// Helper macro to conditionally apply config defaults
/// Only [val] and [opt] fields are configurable in knot.toml
macro_rules! apply_config_if_applicable {
    (val, $self:expr, $defaults:expr, $name:ident) => {
        if $self.$name.is_none() {
            $self.$name = $defaults.$name.clone();
        }
    };
    (opt, $self:expr, $defaults:expr, $name:ident) => {
        if $self.$name.is_none() {
            $self.$name = $defaults.$name.clone();
        }
    };
    (meta, $self:expr, $defaults:expr, $name:ident) => {
        // Skip - metadata is chunk-specific, not configurable in knot.toml
    };
    (col, $self:expr, $defaults:expr, $name:ident) => {
        // Skip - collections are chunk-specific, not configurable in knot.toml
    };
}

macro_rules! define_options {
    (
        $(
            $(#[doc = $doc:expr])*
            $(#[serde($($serde_attr:tt)*)])*
            [$kind:ident] $name:ident : $type:ty , $default:expr
        ),* $(,)?
    ) => {
        /// ChunkOptions with optional fields for YAML parsing.
        #[derive(Debug, Default, Clone, Serialize, Deserialize)]
        #[serde(default)]
        pub struct ChunkOptions {
            $(
                $(#[doc = $doc])*
                $(#[serde($($serde_attr)*)])*
                pub $name: expand_type!($kind, $type),
            )*
        }

        /// ChunkOptions with all values resolved to concrete types.
        #[derive(Debug, Clone, Serialize)]
        pub struct ResolvedChunkOptions {
            $(
                $(#[doc = $doc])*
                pub $name: expand_resolved_type!($kind, $type),
            )*
        }

        impl ChunkOptions {
            /// Resolve all options to concrete values, applying hardcoded defaults.
            pub fn resolve(&self) -> ResolvedChunkOptions {
                #[allow(unused_imports)]
                use crate::defaults::Defaults;
                ResolvedChunkOptions {
                    $(
                        $name: expand_resolve!(self.$name, $kind, $type, $default),
                    )*
                }
            }

            /// Get a ResolvedChunkOptions instance with all default values.
            pub fn default_resolved() -> ResolvedChunkOptions {
                Self::default().resolve()
            }

            /// Apply default values from knot.toml configuration.
            ///
            /// This method is auto-generated by define_options! macro.
            /// Only [val] and [opt] fields are processed; [meta] and [col] fields
            /// are chunk-specific and not configurable in knot.toml.
            pub fn apply_config_defaults(&mut self, defaults: &crate::config::ChunkDefaults) {
                $(
                    apply_config_if_applicable!($kind, self, defaults, $name);
                )*
            }

            /// Returns metadata for all chunk options (used for template generation).
            ///
            /// This method is auto-generated by define_options! macro and provides
            /// all information needed to generate documentation and templates.
            pub fn option_metadata() -> Vec<OptionMetadata> {
                vec![
                    $(
                        OptionMetadata {
                            kind: stringify!($kind),
                            name: stringify!($name),
                            type_name: stringify!($type),
                            default_value: stringify!($default),
                            doc: concat!($($doc),*),
                        },
                    )*
                ]
            }
        }
    }
}

define_options! {
    /// Whether to evaluate the chunk
    [val] eval: bool, true,
    /// What to display in the output (both, input, or output)
    [val] show: Show, Show::Both,
    /// Whether to cache the results
    [val] cache: bool, true,

    /// Optional caption for figures (metadata, not configurable in knot.toml)
    [meta] caption: String, None,
    /// File dependencies
    [col] depends: Vec<PathBuf>, Vec::new(),

    /// Figure width in inches
    #[serde(rename = "fig-width")]
    [val] fig_width: f64, 7.0,
    /// Figure height in inches
    #[serde(rename = "fig-height")]
    [val] fig_height: f64, 5.0,
    /// DPI for raster graphics
    [val] dpi: u32, 300,
    /// Format of plots (svg or png)
    #[serde(rename = "fig-format")]
    [val] fig_format: FigFormat, FigFormat::Svg,

    /// Names of objects to treat as immutable constants
    [col] constant: Vec<String>, Vec::new(),

    // === Presentation Options ===

    /// How to layout input and output when both are displayed (horizontal or vertical)
    [val] layout: Layout, Layout::Horizontal,
    /// Space between input and output blocks (Typst length)
    [opt] gutter: String, None,

    /// Background color for code/input container (Typst color)
    #[serde(rename = "code-background")]
    [opt] code_background: String, None,
    /// Border stroke for code/input container (Typst stroke)
    #[serde(rename = "code-stroke")]
    [opt] code_stroke: String, None,
    /// Corner radius for code/input container (Typst length)
    #[serde(rename = "code-radius")]
    [opt] code_radius: String, None,
    /// Internal padding for code/input container (Typst length)
    #[serde(rename = "code-inset")]
    [opt] code_inset: String, None,

    /// Background color for output container (Typst color)
    #[serde(rename = "output-background")]
    [opt] output_background: String, None,
    /// Border stroke for output container (Typst stroke)
    #[serde(rename = "output-stroke")]
    [opt] output_stroke: String, None,
    /// Corner radius for output container (Typst length)
    #[serde(rename = "output-radius")]
    [opt] output_radius: String, None,
    /// Internal padding for output container (Typst length)
    #[serde(rename = "output-inset")]
    [opt] output_inset: String, None,

    /// Width ratio for horizontal layout (e.g., "1:1", "2:1")
    #[serde(rename = "width-ratio")]
    [opt] width_ratio: String, None,
    /// Content alignment within containers
    [opt] align: String, None,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub language: String,
    pub name: Option<String>,
    pub code: String,
    pub options: ChunkOptions,
    pub codly_options: HashMap<String, String>,
    pub errors: Vec<ChunkError>,
    pub range: Range,      // Position du chunk entier (de ```{r}} à ```)
    pub code_range: Range, // Position du code seul à l'intérieur
    pub start_byte: usize,
    pub end_byte: usize,
    pub code_start_byte: usize,
    pub code_end_byte: usize,
}

macro_rules! define_inline_options {
    (
        $(
            $(#[doc = $doc:expr])*
            $name:ident : $type:ty = $default:expr
        ),* $(,)?
    ) => {
        /// Options for inline expressions
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct InlineOptions {
            $(
                $(#[doc = $doc])*
                pub $name: expand_type!(val, $type),
            )*
        }

        /// Resolved options for inline expressions
        #[derive(Debug, Clone, Serialize)]
        pub struct ResolvedInlineOptions {
            $(
                $(#[doc = $doc])*
                pub $name: expand_resolved_type!(val, $type),
            )*
        }

        impl Default for InlineOptions {
            fn default() -> Self {
                Self {
                    $( $name: None, )*
                }
            }
        }

        impl InlineOptions {
            /// Resolve all options to concrete values, applying defaults.
            pub fn resolve(&self) -> ResolvedInlineOptions {
                ResolvedInlineOptions {
                    $(
                        $name: expand_resolve!(self.$name, val, $type, $default),
                    )*
                }
            }

            /// Get a ResolvedInlineOptions instance with all default values.
            pub fn default_resolved() -> ResolvedInlineOptions {
                Self::default().resolve()
            }
        }
    }
}

define_inline_options! {
    /// Evaluate the code
    eval: bool = true,
    /// What to display (output is default for inline, both/input rarely used)
    show: Show = Show::Output,
    /// Number of digits for numeric formatting
    digits: Option<u32> = None,
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
    pub errors: Vec<ChunkError>,
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
        Ok(doc)
    }
}
