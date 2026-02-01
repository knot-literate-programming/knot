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

    /// Get the R helper path, resolving it relative to the project root
    pub fn r_helper_path(&self, project_root: &Path) -> Option<PathBuf> {
        self.helpers.r.as_ref().map(|r| project_root.join(r))
    }

    /// Get the Typst helper path, resolving it relative to the project root
    pub fn typst_helper_path(&self, project_root: &Path) -> Option<PathBuf> {
        self.helpers.typst.as_ref().map(|t| project_root.join(t))
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

    #[test]
    fn test_load_from_nonexistent_file() {
        // Should return default config without error
        let result = Config::load_from_path("/nonexistent/path/knot.toml");
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.document.main.is_none());
        assert!(config.helpers.r.is_none());
    }

    #[test]
    fn test_load_from_invalid_toml() {
        use std::fs;
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), "this is not valid TOML [[[\n").unwrap();

        let result = Config::load_from_path(temp_file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_r_helper_path() {
        let toml = r#"
[helpers]
r = "lib/knot.R"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        let path = config.r_helper_path();
        assert!(path.is_some());
        assert_eq!(path.unwrap(), PathBuf::from("lib/knot.R"));
    }

    #[test]
    fn test_r_helper_path_none() {
        let config = Config::default();
        assert!(config.r_helper_path().is_none());
    }

    #[test]
    fn test_typst_helper_path() {
        let toml = r#"
[helpers]
typst = "lib/knot.typ"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        let path = config.typst_helper_path();
        assert!(path.is_some());
        assert_eq!(path.unwrap(), PathBuf::from("lib/knot.typ"));
    }

    #[test]
    fn test_typst_helper_path_none() {
        let config = Config::default();
        assert!(config.typst_helper_path().is_none());
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.document.main.is_none());
        assert!(config.helpers.typst.is_none());
        assert!(config.helpers.r.is_none());
    }

    #[test]
    fn test_config_with_all_fields() {
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
    fn test_config_partial_fields() {
        let toml = r#"
[document]
main = "main.knot"
"#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.document.main, Some("main.knot".to_string()));
        assert!(config.helpers.typst.is_none());
        assert!(config.helpers.r.is_none());
    }
}
