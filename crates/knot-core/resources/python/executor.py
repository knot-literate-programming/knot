"""Knot Python Executor - Bridge functions between Rust and Python

This module contains wrapper functions called by the Rust executor.
All executor-specific logic stays in Python, keeping the Rust code simple.
"""

import os


def setup_environment(metadata_file, cache_dir, fig_width, fig_height, fig_dpi, fig_format):
    """Configure environment variables for chunk execution.

    Called by Rust before executing each code chunk to set up the
    side-channel and graphics parameters.

    Args:
        metadata_file: Path to the temporary JSON file for metadata communication
        cache_dir: Path to the cache directory for generated files
        fig_width: Default figure width in inches
        fig_height: Default figure height in inches
        fig_dpi: Default DPI for raster graphics
        fig_format: Default graphics format (svg, png, pdf)
    """
    os.environ['KNOT_METADATA_FILE'] = metadata_file
    os.environ['KNOT_CACHE_DIR'] = cache_dir
    os.environ['KNOT_FIG_WIDTH'] = str(fig_width)
    os.environ['KNOT_FIG_HEIGHT'] = str(fig_height)
    os.environ['KNOT_FIG_DPI'] = str(fig_dpi)
    os.environ['KNOT_FIG_FORMAT'] = fig_format
