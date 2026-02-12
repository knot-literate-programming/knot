// Configuration parsing for knot.toml
//
// Reads the knot.toml configuration file to extract project settings,
// particularly the paths to helper files (Typst and R).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub document: DocumentConfig,
    #[serde(default)]
    pub helpers: HelpersConfig,
    #[serde(default)]
    pub defaults: ChunkDefaults,
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
}

/// Default values for chunk options, configurable in knot.toml
///
/// All fields are optional to allow partial configuration.
/// Priority: chunk options > knot.toml defaults > hardcoded defaults
#[derive(Debug, Default, Deserialize, Clone)]
pub struct ChunkDefaults {
    pub eval: Option<bool>,
    pub show: Option<crate::parser::Show>,
    pub cache: Option<bool>,

    // Graphics options
    #[serde(rename = "fig-width")]
    pub fig_width: Option<f64>,
    #[serde(rename = "fig-height")]
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    #[serde(rename = "fig-format")]
    pub fig_format: Option<crate::parser::FigFormat>,

    // Presentation options
    pub layout: Option<crate::parser::Layout>,
    pub gutter: Option<String>,
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

    // Codly-specific options (extracted from codly-* keys in TOML)
    // Not directly deserialized - populated during post-processing
    #[serde(skip)]
    pub codly_options: HashMap<String, String>,

    // Capture all unknown keys for post-processing
    #[serde(flatten)]
    pub other: HashMap<String, toml::Value>,
}

impl ChunkDefaults {
    /// Extract codly-* options from the "other" HashMap
    ///
    /// This should be called after deserialization to populate codly_options
    pub fn extract_codly_options(&mut self) {
        for (key, value) in &self.other {
            if key.starts_with("codly-") {
                let codly_key = key.strip_prefix("codly-").unwrap().to_string();
                let value_str = match value {
                    toml::Value::String(s) => s.clone(),
                    toml::Value::Boolean(b) => b.to_string(),
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    _ => toml::to_string(value)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                self.codly_options.insert(codly_key, value_str);
            }
        }
    }
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

        let mut config: Config = toml::from_str(&content)
            .context(format!("Failed to parse config file: {}", path.display()))?;

        // Extract codly-* options from language templates
        if let Some(ref mut r_chunks) = config.r_chunks {
            r_chunks.extract_codly_options();
        }
        if let Some(ref mut python_chunks) = config.python_chunks {
            python_chunks.extract_codly_options();
        }
        // Also extract from global defaults
        config.defaults.extract_codly_options();

        Ok(config)
    }

    /// Get the Typst helper path, resolving it relative to the project root
    pub fn typst_helper_path(&self, project_root: &Path) -> Option<PathBuf> {
        self.helpers.typst.as_ref().map(|t| project_root.join(t))
    }

    /// Get language-specific chunk defaults for a given language
    ///
    /// Returns the language-specific defaults if defined in knot.toml,
    /// otherwise returns None.
    ///
    /// # Example
    /// ```toml
    /// [r-chunks]
    /// show = "output"
    /// fig-width = 8.0
    /// ```
    pub fn get_language_defaults(&self, lang: &str) -> Option<&ChunkDefaults> {
        match lang {
            "r" => self.r_chunks.as_ref(),
            "python" => self.python_chunks.as_ref(),
            _ => None,
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
show = "output"
eval = true
cache = true
fig-width = 8.0
fig-height = 6.0
dpi = 600
fig-format = "png"
layout = "vertical"
gutter = "2em"
"##;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.defaults.show, Some(crate::parser::Show::Output));
        assert_eq!(config.defaults.eval, Some(true));
        assert_eq!(config.defaults.cache, Some(true));
        assert_eq!(config.defaults.fig_width, Some(8.0));
        assert_eq!(config.defaults.fig_height, Some(6.0));
        assert_eq!(config.defaults.dpi, Some(600));
        assert_eq!(
            config.defaults.fig_format,
            Some(crate::parser::FigFormat::Png)
        );
        assert_eq!(
            config.defaults.layout,
            Some(crate::parser::Layout::Vertical)
        );
        assert_eq!(config.defaults.gutter, Some("2em".to_string()));
    }

    #[test]
    fn test_defaults_partial() {
        let toml = r#"
[defaults]
show = "output"
fig-width = 10.0
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.defaults.show, Some(crate::parser::Show::Output));
        assert_eq!(config.defaults.fig_width, Some(10.0));
        assert!(config.defaults.eval.is_none());
        assert!(config.defaults.cache.is_none());
    }

    #[test]
    fn test_language_specific_templates() {
        let toml = r##"
[document]
main = "main.knot"

[defaults]
show = "both"
fig-width = 7.0

[r-chunks]
show = "output"
fig-width = 8.0
fig-height = 6.0

[python-chunks]
show = "both"
fig-width = 6.0
dpi = 300
"##;

        let config: Config = toml::from_str(toml).unwrap();

        // Test R-specific defaults
        let r_defaults = config.get_language_defaults("r");
        assert!(r_defaults.is_some());
        let r_defaults = r_defaults.unwrap();
        assert_eq!(r_defaults.show, Some(crate::parser::Show::Output));
        assert_eq!(r_defaults.fig_width, Some(8.0));
        assert_eq!(r_defaults.fig_height, Some(6.0));

        // Test Python-specific defaults
        let python_defaults = config.get_language_defaults("python");
        assert!(python_defaults.is_some());
        let python_defaults = python_defaults.unwrap();
        assert_eq!(python_defaults.show, Some(crate::parser::Show::Both));
        assert_eq!(python_defaults.fig_width, Some(6.0));
        assert_eq!(python_defaults.dpi, Some(300));

        // Test unsupported language returns None
        let julia_defaults = config.get_language_defaults("julia");
        assert!(julia_defaults.is_none());
    }

    #[test]
    fn test_get_language_defaults_none() {
        let config = Config::default();

        // Should return None when no language-specific templates are defined
        assert!(config.get_language_defaults("r").is_none());
        assert!(config.get_language_defaults("python").is_none());
    }

    #[test]
    fn test_language_templates_with_codly_options() {
        let toml = r##"
[document]
main = "main.knot"

[r-chunks]
show = "output"
codly-stroke = '1pt + rgb("#CE412B")'
codly-lang-radius = "10pt"
output-stroke = '1pt + rgb("#CE412B")'

[python-chunks]
show = "both"
codly-zebra-fill = 'rgb("#f0f0f0")'
fig-width = 6.0
"##;

        let content = toml.to_string();
        let mut config: Config = toml::from_str(&content).unwrap();

        // Extract codly options (simulate load_from_path behavior)
        if let Some(ref mut r_chunks) = config.r_chunks {
            r_chunks.extract_codly_options();
        }
        if let Some(ref mut python_chunks) = config.python_chunks {
            python_chunks.extract_codly_options();
        }

        // Test R-chunks codly options
        let r_defaults = config.get_language_defaults("r").unwrap();
        assert_eq!(r_defaults.show, Some(crate::parser::Show::Output));
        assert_eq!(
            r_defaults.codly_options.get("stroke"),
            Some(&"1pt + rgb(\"#CE412B\")".to_string())
        );
        assert_eq!(
            r_defaults.codly_options.get("lang-radius"),
            Some(&"10pt".to_string())
        );
        assert_eq!(
            r_defaults.output_stroke,
            Some("1pt + rgb(\"#CE412B\")".to_string())
        );

        // Test Python-chunks codly options
        let python_defaults = config.get_language_defaults("python").unwrap();
        assert_eq!(python_defaults.show, Some(crate::parser::Show::Both));
        assert_eq!(python_defaults.fig_width, Some(6.0));
        assert_eq!(
            python_defaults.codly_options.get("zebra-fill"),
            Some(&"rgb(\"#f0f0f0\")".to_string())
        );
    }
}
