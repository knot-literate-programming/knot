//! Configuration parsing for `knot.toml`.
//!
//! [`Config::find_and_load`] walks up the directory tree until it finds a
//! `knot.toml`, then deserialises it into a [`Config`] struct.  All fields
//! have sensible defaults so a missing `knot.toml` is not an error.

pub use crate::parser::ast::ChunkDefaults;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Project configuration loaded from `knot.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    /// Explicit executable paths or names for external tools.
    #[serde(default)]
    pub tools: crate::tools::ToolsConfig,
    /// `[document]` section — entry point and include list.
    #[serde(default)]
    pub document: DocumentConfig,
    /// `[execution]` section — timeout and other execution parameters.
    #[serde(default)]
    pub execution: ExecutionConfig,
    /// `[chunk-defaults]` section — global chunk option defaults.
    #[serde(default, rename = "chunk-defaults")]
    pub chunk_defaults: ChunkDefaults,
    /// Codly configuration options (passed to #codly() during initialization)
    #[serde(default)]
    pub codly: HashMap<String, toml::Value>,

    // Language-specific chunk templates
    /// R-specific chunk defaults ([r-chunks] in knot.toml)
    #[serde(default, rename = "r-chunks")]
    pub r_chunks: Option<ChunkDefaults>,
    /// Python-specific chunk defaults ([python-chunks] in knot.toml)
    #[serde(default, rename = "python-chunks")]
    pub python_chunks: Option<ChunkDefaults>,
    /// Warnings found while loading `knot.toml` (unknown keys, deprecated
    /// sections). They are shown at the end of the PDF, on the CLI and in the
    /// editor; the ignored keys do not affect the rest of the configuration.
    #[serde(skip)]
    pub warnings: Vec<String>,
    /// Set when `codly-*` options are used but no source imports codly's
    /// `local()` (see [`crate::codly`]): the options are then ignored.
    #[serde(skip)]
    pub ignore_codly_options: bool,
    /// R-specific error chunk defaults ([r-error] in knot.toml)
    #[serde(default, rename = "r-error")]
    pub r_error: Option<ChunkDefaults>,
    /// Python-specific error chunk defaults ([python-error] in knot.toml)
    #[serde(default, rename = "python-error")]
    pub python_error: Option<ChunkDefaults>,
}

/// `[document]` section of `knot.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DocumentConfig {
    /// Path to the main `.knot` file (e.g. `"main.knot"`).
    pub main: Option<String>,
    /// Additional `.knot` files to compile as includes.
    pub includes: Option<Vec<String>>,
}

/// `[execution]` section of `knot.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExecutionConfig {
    /// Maximum execution time (seconds) for a single R/Python chunk.
    /// If a chunk exceeds this limit, the process is killed and an error is returned.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    crate::defaults::Defaults::DEFAULT_EXECUTION_TIMEOUT_SECS
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            timeout_secs: crate::defaults::Defaults::DEFAULT_EXECUTION_TIMEOUT_SECS,
        }
    }
}

impl Config {
    /// Find and load configuration by searching for knot.toml in parent directories
    pub fn find_and_load(start_path: &Path) -> Result<(Self, PathBuf)> {
        let start_path = std::path::absolute(start_path)?;
        let start_dir = if start_path.is_file() {
            start_path.parent().unwrap_or(&start_path)
        } else {
            &start_path
        };

        let mut current_dir = start_dir.to_path_buf();

        loop {
            let config_path = current_dir.join("knot.toml");

            if config_path.exists() {
                log::info!("Found knot.toml at: {}", config_path.display());
                let config = Self::load_from_path(&config_path)?;
                return Ok((config, current_dir));
            }

            // Move to parent directory
            match current_dir.parent() {
                Some(parent) => current_dir = parent.to_path_buf(),
                None => {
                    // Reached filesystem root without finding knot.toml
                    log::info!("No knot.toml found, using default configuration");
                    return Ok((Self::default(), start_dir.to_path_buf()));
                }
            }
        }
    }

    /// Find project root starting from any path (file or directory)
    pub fn find_project_root(start_path: &Path) -> Result<PathBuf> {
        let (_, project_root) = Self::find_and_load(start_path)?;
        Ok(project_root)
    }

    /// Load configuration from knot.toml in the current directory
    pub fn load() -> Result<Self> {
        Self::load_from_path("knot.toml")
    }

    /// Load configuration from a specific path
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read config file: {}", path.display()))?;

        let mut config: Config = toml::from_str(&content)
            .context(format!("Failed to parse config file: {}", path.display()))?;
        let raw: toml::Table = toml::from_str(&content)
            .context(format!("Failed to parse config file: {}", path.display()))?;
        config.warnings = unknown_config_keys(&raw);

        let root = std::path::absolute(path)?
            .parent()
            .context("Configuration has no parent directory")?
            .to_path_buf();
        config.tools.anchor(&root)?;

        // Extract codly-* options from the chunk sections; report the rest.
        let known: Vec<String> = crate::parser::ChunkOptions::option_metadata()
            .iter()
            .filter(|option| option.kind != "meta")
            .map(|option| option.serde_name())
            .collect();
        let known: Vec<&str> = known.iter().map(String::as_str).collect();
        let sections = [
            ("chunk-defaults", Some(&mut config.chunk_defaults)),
            ("r-chunks", config.r_chunks.as_mut()),
            ("python-chunks", config.python_chunks.as_mut()),
            ("r-error", config.r_error.as_mut()),
            ("python-error", config.python_error.as_mut()),
        ];
        for (section, defaults) in sections {
            let Some(defaults) = defaults else { continue };
            for key in defaults.extract_codly_options() {
                config
                    .warnings
                    .push(unknown_key_message(&key, &format!("[{section}]"), &known));
            }
        }

        Ok(config)
    }

    /// Get language-specific chunk defaults for a given language
    pub fn get_language_defaults(&self, lang: &str) -> Option<&ChunkDefaults> {
        // Parse to Language enum for exhaustive matching
        let language = lang.parse::<crate::defaults::Language>().ok()?;

        match language {
            crate::defaults::Language::R => self.r_chunks.as_ref(),
            crate::defaults::Language::Python => self.python_chunks.as_ref(),
            // Compiler enforces exhaustive matching - adding a new Language
            // variant will cause a compilation error here
        }
    }

    /// Get language-specific error defaults for a given language
    pub fn get_language_error_defaults(&self, lang: &str) -> Option<&ChunkDefaults> {
        // Parse to Language enum for exhaustive matching
        let language = lang.parse::<crate::defaults::Language>().ok()?;

        match language {
            crate::defaults::Language::R => self.r_error.as_ref(),
            crate::defaults::Language::Python => self.python_error.as_ref(),
            // Compiler enforces exhaustive matching - adding a new Language
            // variant will cause a compilation error here
        }
    }
}

const SECTIONS: &[&str] = &[
    "tools",
    "document",
    "execution",
    "chunk-defaults",
    "codly",
    "r-chunks",
    "python-chunks",
    "r-error",
    "python-error",
];

/// Warnings for unknown top-level keys and for unknown keys of `[document]`
/// and `[execution]`. Chunk sections are checked after deserialization, and
/// `[tools]` rejects unknown keys when loading. `[codly]` is passed to codly.
fn unknown_config_keys(raw: &toml::Table) -> Vec<String> {
    let mut warnings = Vec::new();
    for (key, value) in raw {
        if key == "helpers" {
            warnings.push(
                "knot.toml: the [helpers] section is no longer used (the Knot Typst library is embedded) and is ignored."
                    .to_string(),
            );
        } else if !SECTIONS.contains(&key.as_str()) {
            warnings.push(if value.is_table() {
                format!(
                    "knot.toml: unknown section [{key}] is ignored.{}",
                    suggestion(key, SECTIONS)
                        .map_or(String::new(), |s| format!(" Did you mean [{s}]?"))
                )
            } else {
                unknown_key_message(key, "at the top level", SECTIONS)
            });
        }
    }
    for (section, known) in [
        ("document", &["main", "includes"][..]),
        ("execution", &["timeout-secs"][..]),
    ] {
        if let Some(table) = raw.get(section).and_then(toml::Value::as_table) {
            for key in table.keys().filter(|key| !known.contains(&key.as_str())) {
                warnings.push(unknown_key_message(key, &format!("in [{section}]"), known));
            }
        }
    }
    warnings
}

fn unknown_key_message(key: &str, place: &str, known: &[&str]) -> String {
    let place = if place.starts_with('[') {
        format!("in {place}")
    } else {
        place.to_string()
    };
    format!(
        "knot.toml: unknown key '{key}' {place} is ignored.{}",
        suggestion(key, known).map_or(String::new(), |s| format!(" Did you mean '{s}'?"))
    )
}

/// The closest known name, when it is close enough to be a typo.
fn suggestion<'a>(key: &str, known: &[&'a str]) -> Option<&'a str> {
    let key = key.to_lowercase().replace('_', "-");
    known
        .iter()
        .map(|candidate| (edit_distance(&key, candidate), *candidate))
        .filter(|(distance, candidate)| *distance <= 2.max(candidate.len() / 4))
        .min()
        .map(|(_, candidate)| candidate)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (previous + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(current + 1);
            previous = current;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn warnings(toml: &str) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("knot.toml");
        fs::write(&path, toml).unwrap();
        Config::load_from_path(&path).unwrap().warnings
    }

    #[test]
    fn unknown_keys_are_warnings_with_suggestions() {
        let warnings = warnings(
            "[document]\nmain = 'main.knot'\nincludes_ = []\n\n[execution]\ntimeout_secs = 5\n\n[chunk-defaults]\nfig-widht = 5\ncodly-zebra-fill = 'none'\n\n[r-chunks]\ncolour = 'red'\n\n[chunk-defualts]\neval = true\n\n[helpers]\ntypst = 'lib/knot.typ'\n",
        );
        let expected = [
            "knot.toml: unknown section [chunk-defualts] is ignored. Did you mean [chunk-defaults]?",
            "knot.toml: the [helpers] section is no longer used (the Knot Typst library is embedded) and is ignored.",
            "knot.toml: unknown key 'includes_' in [document] is ignored. Did you mean 'includes'?",
            "knot.toml: unknown key 'timeout_secs' in [execution] is ignored. Did you mean 'timeout-secs'?",
            "knot.toml: unknown key 'fig-widht' in [chunk-defaults] is ignored. Did you mean 'fig-width'?",
            "knot.toml: unknown key 'colour' in [r-chunks] is ignored.",
        ];
        let mut sorted = warnings.clone();
        sorted.sort();
        let mut expected: Vec<_> = expected.iter().map(|w| w.to_string()).collect();
        expected.sort();
        assert_eq!(sorted, expected, "{warnings:#?}");
    }

    #[test]
    fn a_valid_configuration_has_no_warning() {
        let toml = "[document]\nmain = 'main.knot'\n\n[execution]\ntimeout-secs = 5\n\n[chunk-defaults]\nfig-width = 5\ncode-stroke = '1pt'\ncodly-zebra-fill = 'none'\n\n[codly]\nanything = 'passed to codly'\n";
        assert_eq!(warnings(toml), Vec::<String>::new());
    }

    #[test]
    fn test_find_and_load() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let project_root = temp.path().join("project");
        let sub_dir = project_root.join("sub/dir");
        fs::create_dir_all(&sub_dir)?;

        let config_path = project_root.join("knot.toml");
        fs::write(&config_path, "[document]\nmain = \"test.knot\"")?;

        let knot_file = sub_dir.join("file.knot");
        fs::write(&knot_file, "content")?;

        let (_config, root) = Config::find_and_load(&sub_dir)?;
        assert_eq!(root, project_root);

        Ok(())
    }
}
