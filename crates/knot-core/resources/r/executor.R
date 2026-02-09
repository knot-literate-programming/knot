# Knot R Executor - Bridge functions between Rust and R
#
# This module contains wrapper functions called by the Rust executor.
# All executor-specific logic stays in R, keeping the Rust code simple.

setup_environment <- function(metadata_file, cache_dir, fig_width, fig_height, fig_dpi, fig_format) {
  # Configure environment variables for chunk execution.
  #
  # Called by Rust before executing each code chunk to set up the
  # side-channel and graphics parameters.
  #
  # Args:
  #   metadata_file: Path to the temporary JSON file for metadata communication
  #   cache_dir: Path to the cache directory for generated files
  #   fig_width: Default figure width in inches
  #   fig_height: Default figure height in inches
  #   fig_dpi: Default DPI for raster graphics
  #   fig_format: Default graphics format (svg, png, pdf)

  Sys.setenv(KNOT_METADATA_FILE = metadata_file)
  Sys.setenv(KNOT_CACHE_DIR = cache_dir)
  Sys.setenv(KNOT_FIG_WIDTH = as.character(fig_width))
  Sys.setenv(KNOT_FIG_HEIGHT = as.character(fig_height))
  Sys.setenv(KNOT_FIG_DPI = as.character(fig_dpi))
  Sys.setenv(KNOT_FIG_FORMAT = fig_format)
}
