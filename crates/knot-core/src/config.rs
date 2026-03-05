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
    /// `[document]` section — entry point and include list.
    #[serde(default)]
    pub document: DocumentConfig,
    /// `[helpers]` section — optional custom Typst helper path.
    #[serde(default)]
    pub helpers: HelpersConfig,
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

/// `[helpers]` section of `knot.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HelpersConfig {
    /// Path to a custom Typst helper file (relative to project root).
    pub typst: Option<String>,
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
        let start_dir = if start_path.is_file() {
            start_path.parent().unwrap_or(start_path)
        } else {
            start_path
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

        // Extract codly-* options from language templates
        if let Some(ref mut r_chunks) = config.r_chunks {
            r_chunks.extract_codly_options();
        }
        if let Some(ref mut python_chunks) = config.python_chunks {
            python_chunks.extract_codly_options();
        }
        if let Some(ref mut r_error) = config.r_error {
            r_error.extract_codly_options();
        }
        if let Some(ref mut python_error) = config.python_error {
            python_error.extract_codly_options();
        }
        // Also extract from global defaults
        config.chunk_defaults.extract_codly_options();

        Ok(config)
    }

    /// Get the Typst helper path, resolving it relative to the project root
    pub fn typst_helper_path(&self, project_root: &Path) -> Option<PathBuf> {
        self.helpers.typst.as_ref().map(|t| project_root.join(t))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
