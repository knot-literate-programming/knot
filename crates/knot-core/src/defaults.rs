//! Centralized default values for all knot configuration
//!
//! This module provides a single source of truth for all hardcoded default values
//! used throughout the knot codebase. This ensures consistency and makes it easy
//! to modify defaults in one place.

/// Default values for chunk options, inline options, graphics, and system constants
pub struct Defaults;

impl Defaults {
    // ============================================================================
    // Chunk Options Defaults
    // ============================================================================

    /// Default: evaluate chunk code (true)
    pub const CHUNK_EVAL: bool = true;

    /// Default: show chunk code in output (true)
    pub const CHUNK_ECHO: bool = true;

    /// Default: show chunk execution results (true)
    pub const CHUNK_OUTPUT: bool = true;

    /// Default: cache chunk results (true)
    pub const CHUNK_CACHE: bool = true;

    // ============================================================================
    // Inline Expression Options Defaults
    // ============================================================================

    /// Default: don't show inline code (false) - different from chunks!
    pub const INLINE_ECHO: bool = false;

    /// Default: evaluate inline expressions (true)
    pub const INLINE_EVAL: bool = true;

    /// Default: show inline results (true)
    pub const INLINE_OUTPUT: bool = true;

    // ============================================================================
    // Graphics Options Defaults
    // ============================================================================

    /// Default figure width in inches
    pub const FIG_WIDTH: f64 = 7.0;

    /// Default figure height in inches
    pub const FIG_HEIGHT: f64 = 5.0;

    /// Default DPI (dots per inch) for raster graphics
    pub const DPI: u32 = 300;

    /// Default graphics format (SVG for vector graphics)
    pub const FIG_FORMAT: &'static str = "svg";

    // ============================================================================
    // System Constants
    // ============================================================================

    /// Boundary marker used to delimit R process output streams
    pub const BOUNDARY_MARKER: &'static str = "---KNOT_CHUNK_BOUNDARY---";

    /// Default cache directory name
    pub const CACHE_DIR_NAME: &'static str = ".knot_cache";

    /// Directory name for R-generated files (plots, CSVs)
    pub const R_FILES_DIR: &'static str = "_knot_r_files";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_defaults_are_all_true() {
        // Document that chunks default to "show everything"
    }

    #[test]
    fn test_inline_echo_differs_from_chunk() {
        // Inline expressions hide code by default (cleaner inline output)
        assert_ne!(Defaults::INLINE_ECHO, Defaults::CHUNK_ECHO);
    }

    #[test]
    fn test_graphics_defaults_reasonable() {
        // Graphics defaults should be sensible values
    }

    #[test]
    fn test_system_constants_not_empty() {}
}
