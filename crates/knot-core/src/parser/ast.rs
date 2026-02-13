use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// Chunk Option Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    #[default]
    Both,
    Code,
    Output,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WarningsVisibility {
    #[default]
    Below,
    Inline,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FigFormat {
    #[default]
    Svg,
    Png,
}

impl FigFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            FigFormat::Svg => "svg",
            FigFormat::Png => "png",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkError {
    pub message: String,
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

#[derive(Debug, Clone)]
pub struct OptionMetadata {
    pub kind: &'static str,
    pub name: &'static str,
    pub type_name: &'static str,
    pub default_value: &'static str,
    pub doc: &'static str,
}

impl OptionMetadata {
    pub fn serde_name(&self) -> String {
        self.name.replace('_', "-")
    }
}

// ============================================================================
// Chunk Options Macro System (Unified, Robust, Formatting Aware)
// ============================================================================

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

macro_rules! apply_merge {
    (col, $self:expr, $other:expr, $name:ident) => {
        // Skip collections in merge
    };
    ($kind:ident, $self:expr, $other:expr, $name:ident) => {
        if $other.$name.is_some() {
            $self.$name = $other.$name.clone();
        }
    };
}

macro_rules! apply_config {
    (col, $self:expr, $defaults:expr, $name:ident) => {
        // Skip collections in config
    };
    ($kind:ident, $self:expr, $defaults:expr, $name:ident) => {
        if $self.$name.is_none() {
            $self.$name = $defaults.$name.clone();
        }
    };
}

macro_rules! format_field {
    (col, $self:expr, $name:ident, $serde_name:expr) => {
        if !$self.$name.is_empty() {
            let yaml_val = serde_yaml::to_string(&$self.$name).unwrap_or_default().trim().to_string();
            format!("#| {}: {}\n", $serde_name, yaml_val)
        } else {
            String::new()
        }
    };
    ($kind:ident, $self:expr, $name:ident, $serde_name:expr) => {
        if let Some(ref val) = $self.$name {
            let yaml_val = match serde_yaml::to_value(val) {
                Ok(serde_yaml::Value::String(s)) => s,
                Ok(v) => serde_yaml::to_string(&v).unwrap_or_default().trim().to_string(),
                Err(_) => String::new(),
            };
            // Note: Even if yaml_val is "none", we write it because it overrides defaults.
            format!("#| {}: {}\n", $serde_name, yaml_val)
        } else {
            String::new()
        }
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
        #[derive(Debug, Default, Clone, Serialize, Deserialize)]
        #[serde(default)]
        pub struct ChunkOptions {
            $(
                $(#[doc = $doc])*
                $(#[serde($($serde_attr)*)])*
                pub $name: expand_type!($kind, $type),
            )*
        }

        #[derive(Debug, Default, Clone, Serialize, Deserialize)]
        #[serde(default)]
        pub struct ChunkDefaults {
            $(
                $(#[doc = $doc])*
                $(#[serde($($serde_attr)*)])*
                pub $name: expand_type!($kind, $type),
            )*

            #[serde(skip)]
            pub codly_options: HashMap<String, String>,

            #[serde(flatten)]
            pub other: HashMap<String, toml::Value>,
        }

        #[derive(Debug, Clone, Serialize)]
        pub struct ResolvedChunkOptions {
            $(
                $(#[doc = $doc])*
                pub $name: expand_resolved_type!($kind, $type),
            )*
        }

        impl ChunkDefaults {
            pub fn merge(&mut self, other: &ChunkDefaults) {
                $(
                    apply_merge!($kind, self, other, $name);
                )*
                for (key, value) in &other.codly_options {
                    self.codly_options.insert(key.clone(), value.clone());
                }
            }

            pub fn extract_codly_options(&mut self) {
                for (key, value) in &self.other {
                    if key.starts_with("codly-") {
                        let codly_key = key.strip_prefix("codly-").unwrap().to_string();
                        let value_str = match value {
                            toml::Value::String(s) => s.clone(),
                            toml::Value::Boolean(b) => b.to_string(),
                            toml::Value::Integer(i) => i.to_string(),
                            toml::Value::Float(f) => f.to_string(),
                            _ => toml::to_string(value).unwrap_or_default().trim().to_string(),
                        };
                        self.codly_options.insert(codly_key, value_str);
                    }
                }
            }
        }

        impl ChunkOptions {
            pub fn resolve(&self) -> ResolvedChunkOptions {
                ResolvedChunkOptions {
                    $(
                        $name: expand_resolve!(self.$name, $kind, $type, $default),
                    )*
                }
            }

            pub fn default_resolved() -> ResolvedChunkOptions {
                Self::default().resolve()
            }

            pub fn apply_config_defaults(&mut self, defaults: &ChunkDefaults) {
                $(
                    apply_config!($kind, self, defaults, $name);
                )*
            }

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

            /// Format present options back to Quarto-style YAML block (#| key: value)
            pub fn format_to_quarto(&self) -> String {
                let mut out = String::new();
                let meta = Self::option_metadata();
                
                $(
                    let serde_name = meta.iter().find(|m| m.name == stringify!($name)).unwrap().serde_name();
                    out.push_str(&format_field!($kind, self, $name, serde_name));
                )*
                out
            }
        }
    };
}

define_options! {
    /// Whether to evaluate the chunk
    [val] eval: bool, true,
    /// What to display in the output (both, code, or output)
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

    /// How to layout code and output when both are displayed (horizontal or vertical)
    [val] layout: Layout, Layout::Horizontal,
    /// Where to display warnings: below the block, inline within the layout, or none
    #[serde(rename = "warnings-visibility")]
    [val] warnings_visibility: WarningsVisibility, WarningsVisibility::Below,
    /// Space between code and output blocks (Typst length)
    [opt] gutter: String, None,

    /// Background color for code container (Typst color)
    #[serde(rename = "code-background")]
    [opt] code_background: String, None,
    /// Border stroke for code container (Typst stroke)
    #[serde(rename = "code-stroke", alias = "code-border")]
    [opt] code_stroke: String, None,
    /// Corner radius for code container (Typst length)
    #[serde(rename = "code-radius")]
    [opt] code_radius: String, None,
    /// Internal padding for code container (Typst length)
    #[serde(rename = "code-inset", alias = "code-padding")]
    [opt] code_inset: String, None,

    /// Background color for output container (Typst color)
    #[serde(rename = "output-background")]
    [opt] output_background: String, None,
    /// Border stroke for output container (Typst stroke)
    #[serde(rename = "output-stroke", alias = "output-border")]
    [opt] output_stroke: String, None,
    /// Corner radius for output container (Typst length)
    #[serde(rename = "output-radius")]
    [opt] output_radius: String, None,
    /// Internal padding for output container (Typst length)
    #[serde(rename = "output-inset", alias = "output-padding")]
    [opt] output_inset: String, None,

    /// Background color for warning container (Typst color)
    #[serde(rename = "warning-background")]
    [opt] warning_background: String, None,
    /// Border stroke for warning container (Typst stroke)
    #[serde(rename = "warning-stroke", alias = "warning-border")]
    [opt] warning_stroke: String, None,
    /// Corner radius for warning container (Typst length)
    #[serde(rename = "warning-radius")]
    [opt] warning_radius: String, None,
    /// Internal padding for warning container (Typst length)
    #[serde(rename = "warning-inset", alias = "warning-padding")]
    [opt] warning_inset: String, None,

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
    pub range: Range,
    pub code_range: Range,
    pub start_byte: usize,
    pub end_byte: usize,
    pub code_start_byte: usize,
    pub code_end_byte: usize,
}

impl Chunk {
    /// Format the chunk back to its canonical source representation
    pub fn format(&self) -> String {
        let mut out = String::new();

        // 1. Header: ```{lang name}
        out.push_str("```{");
        out.push_str(&self.language);
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                out.push(' ');
                out.push_str(name);
            }
        }
        out.push_str("}\n");

        // 2. Options: #| key: value
        let options_yaml = self.options.format_to_quarto();
        out.push_str(&options_yaml);

        // 3. Codly options: #| codly-key: value
        let mut codly_keys: Vec<_> = self.codly_options.keys().collect();
        codly_keys.sort(); // Deterministic order
        let mut has_options = !options_yaml.is_empty();
        for key in codly_keys {
            let val = self.codly_options.get(key).unwrap();
            out.push_str(&format!("#| codly-{}: {}\n", key, val));
            has_options = true;
        }

        // Add a blank line between options and code if options exist
        if has_options {
            out.push('\n');
        }

        // 4. Code: (Trim to ensure clean boundaries)
        let trimmed_code = self.code.trim();
        if !trimmed_code.is_empty() {
            out.push_str(trimmed_code);
            out.push('\n');
        }

        // 5. Footer
        out.push_str("```");

        out
    }
}

macro_rules! define_inline_options {
    (
        $(
            $(#[doc = $doc:expr])*
            $name:ident : $type:ty = $default:expr
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct InlineOptions {
            $(
                $(#[doc = $doc])*
                pub $name: expand_type!(val, $type),
            )*
        }

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
            pub fn resolve(&self) -> ResolvedInlineOptions {
                ResolvedInlineOptions {
                    $(
                        $name: expand_resolve!(self.$name, val, $type, $default),
                    )*
                }
            }
        }
    }
}

define_inline_options! {
    eval: bool = true,
    show: Show = Show::Output,
    digits: Option<u32> = None,
}

#[derive(Debug, Clone)]
pub struct InlineExpr {
    pub language: String,
    pub code: String,
    pub start: usize,
    pub end: usize,
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
    pub fn parse(source: String) -> Result<Self> {
        let doc = super::winnow_parser::parse_document(&source);
        Ok(doc)
    }
}
