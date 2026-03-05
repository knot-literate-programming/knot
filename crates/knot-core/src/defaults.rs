//! Centralized default values for all knot configuration
//!
//! This module provides a single source of truth for all hardcoded default values
//! used throughout the knot codebase. This ensures consistency and makes it easy
//! to modify defaults in one place.

use std::fmt;
use std::str::FromStr;

/// Supported programming languages for code execution.
///
/// This enum is the single source of truth for supported languages.
/// Adding a new language requires:
/// 1. Adding a variant here
/// 2. Updating the `match` in `ExecutorManager::get_executor()`
/// 3. Updating the `match` in `Config::get_language_defaults()` and `get_language_error_defaults()`
///
/// The compiler will enforce updating all three locations via exhaustive pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// The R statistical computing language.
    R,
    /// The Python programming language.
    Python,
}

impl Language {
    /// Returns all supported languages
    pub fn all() -> &'static [Language] {
        &[Language::R, Language::Python]
    }

    /// Returns the lowercase string representation of the language
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::R => "r",
            Language::Python => "python",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "r" => Ok(Language::R),
            "python" | "py" => Ok(Language::Python),
            _ => Err(format!("Unsupported language: '{}'", s)),
        }
    }
}

/// Default values for chunk options, inline options, graphics, and system constants
pub struct Defaults;

impl Defaults {
    // ============================================================================
    // System Constants
    // ============================================================================

    /// Boundary marker used to delimit R process output streams
    pub const BOUNDARY_MARKER: &'static str = "---KNOT_CHUNK_BOUNDARY---";

    /// Default cache directory name
    pub const CACHE_DIR_NAME: &'static str = ".knot_cache";

    /// Directory name for language-generated files (plots, CSVs)
    pub const LANGUAGE_FILES_DIR: &'static str = "_knot_files";

    /// List of supported languages for code chunks (as string slices)
    pub const SUPPORTED_LANGUAGES: &[&str] = &["r", "python"];

    /// Returns all supported languages as Language enum values
    pub fn supported_languages() -> &'static [Language] {
        Language::all()
    }

    // ============================================================================
    // Execution Constants
    // ============================================================================

    /// Default timeout (in seconds) for R/Python chunk execution.
    /// Overridable via `[execution] timeout-secs` in knot.toml.
    pub const DEFAULT_EXECUTION_TIMEOUT_SECS: u64 = 30;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_constants_not_empty() {
        // Verify constants have expected non-empty values
        assert_ne!(Defaults::BOUNDARY_MARKER, "");
        assert_ne!(Defaults::CACHE_DIR_NAME, "");
    }
}
