// Configuration parsing for knot.toml
//
// Reads the knot.toml configuration file to extract project settings,
// particularly the paths to helper files (Typst and R).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub document: DocumentConfig,
    #[serde(default)]
    pub helpers: HelpersConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct DocumentConfig {
    pub main: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HelpersConfig {
    pub typst: Option<String>,
    pub r: Option<String>,
}

impl Config {
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

    /// Get the R helper path, resolving it relative to the config file location
    pub fn r_helper_path(&self) -> Option<PathBuf> {
        self.helpers.r.as_ref().map(PathBuf::from)
    }

    /// Get the Typst helper path, resolving it relative to the config file location
    pub fn typst_helper_path(&self) -> Option<PathBuf> {
        self.helpers.typst.as_ref().map(PathBuf::from)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            document: DocumentConfig::default(),
            helpers: HelpersConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
[document]
main = "main.knot"

[helpers]
typst = "lib/knot.typ"
r = "lib/knot.R"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.document.main, Some("main.knot".to_string()));
        assert_eq!(config.helpers.typst, Some("lib/knot.typ".to_string()));
        assert_eq!(config.helpers.r, Some("lib/knot.R".to_string()));
    }

    #[test]
    fn test_parse_empty_config() {
        let toml = "";
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.document.main.is_none());
        assert!(config.helpers.r.is_none());
    }

    #[test]
    fn test_parse_package_config() {
        let toml = r#"
[helpers]
typst = "@preview/knot"
r = "@cran/knot"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.helpers.typst, Some("@preview/knot".to_string()));
        assert_eq!(config.helpers.r, Some("@cran/knot".to_string()));
    }
}
