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

/// Canonical name of a language tag as written in a chunk header or inline
/// expression (`py`, `Python` → `python`, `R` → `r`).
///
/// Unknown tags are returned unchanged, so that non-executed chunks in other
/// languages keep their name; they are reported as unsupported when run.
pub fn canonical_language(tag: &str) -> String {
    tag.parse::<Language>()
        .map(|language| language.as_str().to_string())
        .unwrap_or_else(|_| tag.to_string())
}

/// Error shown in the PDF and in the editor for a language Knot cannot
/// execute; `None` when the language is supported.
pub fn unsupported_language_message(language: &str) -> Option<String> {
    language.parse::<Language>().err()?;
    let supported: Vec<_> = Language::all().iter().map(Language::as_str).collect();
    Some(format!(
        "Unsupported language '{language}': Knot executes {}. Add '#| eval: false' to display this code without running it.",
        supported.join(" and ")
    ))
}

/// Warning shown in the PDF and in the editor on the first chunk of a language
/// chain whose session could not be saved as a reusable snapshot.
pub fn non_reusable_snapshot_message(language: &str) -> String {
    format!(
        "The {language} session after this chunk cannot be saved as a reusable snapshot: it holds objects that cannot be serialized, such as functions, classes or instances defined in the document. This chunk and the following {language} chunks are therefore re-executed at every compilation. Delete such objects once they are no longer needed (with del), or disable {language} snapshots in the document header (snapshots: {{{language}: false}})."
    )
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

    /// Minimum time allowed for an interpreter to start and load the Knot
    /// helpers. Startup is not governed by the per-chunk timeout: a short
    /// `timeout-secs` must not prevent R or Python from starting on a slow machine.
    pub const INTERPRETER_STARTUP_TIMEOUT_SECS: u64 = 60;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_from_str_accepts_supported_names_and_aliases() {
        for (input, expected) in [
            ("r", Language::R),
            ("R", Language::R),
            ("python", Language::Python),
            ("Python", Language::Python),
            ("PYTHON", Language::Python),
            ("py", Language::Python),
            ("Py", Language::Python),
        ] {
            assert_eq!(input.parse::<Language>(), Ok(expected), "input: {input:?}");
        }
    }

    #[test]
    fn language_from_str_rejects_unknown_empty_and_padded_names() {
        for input in ["julia", "", "R ", " python", "py\n"] {
            assert!(input.parse::<Language>().is_err(), "input: {input:?}");
        }
    }

    #[test]
    fn test_system_constants_not_empty() {
        // Verify constants have expected non-empty values
        assert_ne!(Defaults::BOUNDARY_MARKER, "");
        assert_ne!(Defaults::CACHE_DIR_NAME, "");
    }
}

#[cfg(test)]
mod startup_tests {
    use std::time::Duration;

    #[test]
    fn startup_is_never_limited_by_a_short_chunk_timeout() {
        let startup = crate::executors::startup_timeout;
        let long = Duration::from_secs(600);
        assert_eq!(
            startup(Duration::from_millis(500)),
            Duration::from_secs(super::Defaults::INTERPRETER_STARTUP_TIMEOUT_SECS)
        );
        assert_eq!(startup(long), long);
    }
}
