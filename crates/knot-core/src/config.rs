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

    // Presentation options
    pub layout: Option<String>,
    pub gutter: Option<String>,
    #[serde(rename = "code-background")]
    pub code_background: Option<String>,
    #[serde(rename = "code-stroke")]
    pub code_stroke: Option<String>,
    #[serde(rename = "code-radius")]
    pub code_radius: Option<String>,
    #[serde(rename = "code-inset")]
    pub code_inset: Option<String>,
    #[serde(rename = "output-background")]
    pub output_background: Option<String>,
    #[serde(rename = "output-stroke")]
    pub output_stroke: Option<String>,
    #[serde(rename = "output-radius")]
    pub output_radius: Option<String>,
    #[serde(rename = "output-inset")]
    pub output_inset: Option<String>,
    #[serde(rename = "width-ratio")]
    pub width_ratio: Option<String>,
    pub align: Option<String>,
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
    /// Starts from `start_path` (file or directory) and walks up the directory tree
    /// until it finds knot.toml. This mimics Cargo's behavior for finding Cargo.toml.
    ///
    /// Returns:
    /// - Ok((config, project_root)) if knot.toml is found
    /// - Ok((default_config, start_dir)) if no knot.toml is found
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
    ///
    /// This is a convenience wrapper around `find_and_load()` that automatically
    /// handles both file and directory paths.
    ///
    /// Returns the project root directory (containing knot.toml)
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

        // Test with directory
        let (config, root) = Config::find_and_load(&sub_dir)?;
        assert_eq!(root, project_root);
        assert_eq!(config.document.main, Some("test.knot".to_string()));

        // Test with file
        let (config, root) = Config::find_and_load(&knot_file)?;
        assert_eq!(root, project_root);
        assert_eq!(config.document.main, Some("test.knot".to_string()));

        Ok(())
    }

    #[test]
    fn test_defaults_section() {
        let toml = r##"
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
layout = "vertical"
code-background = "#f5f5f5"
gutter = "2em"
"##;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.defaults.echo, Some(false));
        assert_eq!(config.defaults.eval, Some(true));
        assert_eq!(config.defaults.cache, Some(true));
        assert_eq!(config.defaults.fig_width, Some(8.0));
        assert_eq!(config.defaults.fig_height, Some(6.0));
        assert_eq!(config.defaults.dpi, Some(600));
        assert_eq!(config.defaults.fig_format, Some("png".to_string()));
        assert_eq!(config.defaults.layout, Some("vertical".to_string()));
        assert_eq!(config.defaults.code_background, Some("#f5f5f5".to_string()));
        assert_eq!(config.defaults.gutter, Some("2em".to_string()));
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
