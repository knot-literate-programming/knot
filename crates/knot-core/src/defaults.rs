//! Centralized default values for all knot configuration
//!
//! This module provides a single source of truth for all hardcoded default values
//! used throughout the knot codebase. This ensures consistency and makes it easy
//! to modify defaults in one place.

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

    /// List of supported languages for code chunks
    pub const SUPPORTED_LANGUAGES: &[&str] = &["r", "python"];

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
    #[allow(clippy::const_is_empty)]
    fn test_system_constants_not_empty() {
        // Verify constants have expected non-empty values
        assert!(!Defaults::BOUNDARY_MARKER.is_empty());
        assert!(!Defaults::CACHE_DIR_NAME.is_empty());
    }
}
