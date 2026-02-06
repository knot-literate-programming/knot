// Configuration parsing for knot.toml
//
// Reads the knot.toml configuration file to extract project settings,
// particularly the paths to helper files (Typst and R).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub document: DocumentConfig,
    #[serde(default)]
    pub helpers: HelpersConfig,
    #[serde(default)]
    pub defaults: ChunkDefaults,
}

/// Default values for chunk options, configurable in knot.toml
///
/// All fields are optional to allow partial configuration.
/// Priority: chunk options > knot.toml defaults > hardcoded defaults
#[derive(Debug, Default, Deserialize)]
pub struct ChunkDefaults {
    pub eval: Option<bool>,
    pub echo: Option<bool>,
    pub output: Option<bool>,
    pub cache: Option<bool>,

    // Graphics options
    #[serde(rename = "fig-width")]
    pub fig_width: Option<f64>,
    #[serde(rename = "fig-height")]
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    #[serde(rename = "fig-format")]
    pub fig_format: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DocumentConfig {
    pub main: Option<String>,
    pub includes: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HelpersConfig {
    pub typst: Option<String>,
}

impl Config {
    /// Find and load configuration by searching for knot.toml in parent directories
    ///
    /// Starts from `start_dir` and walks up the directory tree until it finds knot.toml.
    /// This mimics Cargo's behavior for finding Cargo.toml.
    ///
    /// Returns:
    /// - Ok((config, project_root)) if knot.toml is found
    /// - Ok((default_config, start_dir)) if no knot.toml is found
    pub fn find_and_load(start_dir: &Path) -> Result<(Self, PathBuf)> {
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

    /// Load configuration from knot.toml in the current directory
    pub fn load() -> Result<Self> {
        Self::load_from_path("knot.toml")
    }

    /// Load configuration from a specific path
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            // If knot.toml doesn't exist, return default config
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .context(format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Get the Typst helper path, resolving it relative to the project root
    pub fn typst_helper_path(&self, project_root: &Path) -> Option<PathBuf> {
        self.helpers.typst.as_ref().map(|t| project_root.join(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_section() {
        let toml = r#"
[document]
main = "main.knot"

[defaults]
echo = false
eval = true
cache = true
fig-width = 8.0
fig-height = 6.0
dpi = 600
fig-format = "png"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.defaults.echo, Some(false));
        assert_eq!(config.defaults.eval, Some(true));
        assert_eq!(config.defaults.cache, Some(true));
        assert_eq!(config.defaults.fig_width, Some(8.0));
        assert_eq!(config.defaults.fig_height, Some(6.0));
        assert_eq!(config.defaults.dpi, Some(600));
        assert_eq!(config.defaults.fig_format, Some("png".to_string()));
    }

    #[test]
    fn test_defaults_partial() {
        let toml = r#"
[defaults]
echo = false
fig-width = 10.0
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.defaults.echo, Some(false));
        assert_eq!(config.defaults.fig_width, Some(10.0));
        assert!(config.defaults.eval.is_none());
        assert!(config.defaults.cache.is_none());
    }
}
