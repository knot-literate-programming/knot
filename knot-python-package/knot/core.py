"""
Knot - Python support for literate programming with Typst

This module provides functions to convert Python objects (DataFrames, plots)
to Typst-compatible output via side-channel communication.
"""

import os
import json
import hashlib
from pathlib import Path
from typing import Any, Optional


def _get_base_dir() -> Path:
    """Get cache directory from environment or temp.

    Priority:
    1. KNOT_CACHE_DIR environment variable
    2. tempfile.gettempdir() as fallback

    Returns:
        Path: Cache directory path
    """
    cache_dir = os.environ.get('KNOT_CACHE_DIR')
    if cache_dir:
        cache_path = Path(cache_dir)
        cache_path.mkdir(parents=True, exist_ok=True)
        return cache_path

    import tempfile
    return Path(tempfile.gettempdir())


def _write_metadata(metadata: dict) -> bool:
    """Write metadata to side-channel file.

    Appends metadata to the side-channel file if KNOT_METADATA_FILE is set.

    Args:
        metadata: Dictionary representing metadata entry

    Returns:
        bool: True if metadata was written, False otherwise
    """
    metadata_file = os.environ.get('KNOT_METADATA_FILE')
    if not metadata_file:
        return False

    filepath = Path(metadata_file)

    # Read existing metadata
    existing = []
    if filepath.exists():
        try:
            with open(filepath, 'r') as f:
                content = f.read().strip()
                if content:
                    existing = json.loads(content)
        except (json.JSONDecodeError, IOError):
            existing = []

    # Append new metadata
    existing.append(metadata)

    # Write back as JSON array
    with open(filepath, 'w') as f:
        json.dump(existing, f)

    return True


def typst(obj: Any, **kwargs) -> Any:
    """Convert Python objects to Typst representations.

    Generic function to convert Python objects (DataFrames, plots, etc.)
    to Typst-compatible output via side-channel or serialization.

    Supports:
    - matplotlib.figure.Figure (and subclasses)
    - plotnine.ggplot
    - pandas.DataFrame

    Args:
        obj: Python object to convert
        **kwargs: Additional arguments passed to type-specific handlers
            For plots: width, height, dpi, format
            For DataFrames: index (default False)

    Returns:
        The original object (for chaining)

    Examples:
        >>> import pandas as pd
        >>> from knot import typst
        >>>
        >>> # DataFrame
        >>> df = pd.DataFrame({'x': [1, 2, 3], 'y': [4, 5, 6]})
        >>> typst(df)
        >>>
        >>> # Matplotlib
        >>> import matplotlib.pyplot as plt
        >>> fig, ax = plt.subplots()
        >>> ax.plot([1, 2, 3])
        >>> typst(fig)
    """
    # Try matplotlib Figure
    try:
        import matplotlib.figure
        if isinstance(obj, matplotlib.figure.Figure):
            return _typst_matplotlib(obj, **kwargs)
    except ImportError:
        pass

    # Try plotnine
    try:
        from plotnine import ggplot
        if isinstance(obj, ggplot):
            return _typst_plotnine(obj, **kwargs)
    except ImportError:
        pass

    # Try pandas DataFrame
    try:
        import pandas as pd
        if isinstance(obj, pd.DataFrame):
            return _typst_dataframe(obj, **kwargs)
    except ImportError:
        pass

    # Fallback: just print
    print(obj)
    return obj


def current_plot():
    """Get the current matplotlib figure.

    Captures the current figure using plt.gcf() (get current figure).
    This is a convenience wrapper for use with typst().

    Returns:
        matplotlib.figure.Figure: The current figure

    Raises:
        RuntimeError: If matplotlib is not available
        ValueError: If no active figure exists or figure is empty

    Examples:
        >>> import matplotlib.pyplot as plt
        >>> from knot import typst, current_plot
        >>>
        >>> plt.plot([1, 2, 3], [1, 4, 9])
        >>> plt.title("My Plot")
        >>> typst(current_plot())
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError:
        raise RuntimeError(
            "matplotlib is required for current_plot(). "
            "Install it with: pip install matplotlib"
        )

    fig = plt.gcf()

    # Check if figure has content (at least one axes)
    if not fig.get_axes():
        raise ValueError(
            "Current figure is empty. Create a plot first.\n"
            "Example: plt.plot([1, 2, 3])"
        )

    return fig


def _typst_matplotlib(
    fig,
    width: Optional[float] = None,
    height: Optional[float] = None,
    dpi: Optional[int] = None,
    format: Optional[str] = None
):
    """Save matplotlib figure via side-channel.

    Args:
        fig: matplotlib Figure object
        width: Figure width in inches (default: from KNOT_FIG_WIDTH or 7)
        height: Figure height in inches (default: from KNOT_FIG_HEIGHT or 5)
        dpi: Resolution in DPI (default: from KNOT_FIG_DPI or 300)
        format: Output format - 'svg', 'png', 'pdf' (default: from KNOT_FIG_FORMAT or 'svg')

    Returns:
        The matplotlib Figure object
    """
    import matplotlib.pyplot as plt
    import io

    # Read defaults from environment (set by knot from chunk options)
    width = width or float(os.environ.get('KNOT_FIG_WIDTH', '7'))
    height = height or float(os.environ.get('KNOT_FIG_HEIGHT', '5'))
    dpi = dpi or int(os.environ.get('KNOT_FIG_DPI', '300'))
    format = format or os.environ.get('KNOT_FIG_FORMAT', 'svg')

    # Set figure size
    fig.set_size_inches(width, height)

    # Hash the figure for unique filename
    # We hash the rendered SVG for stable, content-based hashing
    buf = io.BytesIO()
    fig.savefig(buf, format='svg', bbox_inches='tight')
    fig_hash = hashlib.sha256(buf.getvalue()).hexdigest()[:16]

    filename = f"plot_{fig_hash}.{format}"
    filepath = _get_base_dir() / filename

    # Save figure with specified format
    fig.savefig(
        filepath,
        format=format,
        dpi=dpi,
        bbox_inches='tight'
    )

    # Write metadata via side-channel
    metadata = {
        'type': 'plot',
        'path': str(filepath.absolute()),
        'format': format
    }

    if not _write_metadata(metadata):
        # Not in knot environment, show plot normally
        plt.show()

    return fig


def _typst_plotnine(gg, **kwargs):
    """Save plotnine plot (delegates to matplotlib).

    plotnine builds on matplotlib, so we extract the underlying Figure
    and delegate to _typst_matplotlib.

    Args:
        gg: plotnine.ggplot object
        **kwargs: Additional arguments passed to _typst_matplotlib

    Returns:
        The plotnine ggplot object
    """
    # plotnine.ggplot.draw() returns a matplotlib Figure
    fig = gg.draw()
    _typst_matplotlib(fig, **kwargs)
    return gg


def _typst_dataframe(df, index: bool = False, **kwargs):
    """Save pandas DataFrame as CSV via side-channel.

    Args:
        df: pandas DataFrame object
        index: Include index in CSV (default: False)
        **kwargs: Additional arguments (currently unused)

    Returns:
        The pandas DataFrame object
    """
    # Hash DataFrame content for unique filename
    # Use a stable representation of the data
    df_string = df.to_string()
    df_hash = hashlib.sha256(df_string.encode()).hexdigest()[:16]

    filename = f"dataframe_{df_hash}.csv"
    filepath = _get_base_dir() / filename

    # Save CSV
    df.to_csv(filepath, index=index)

    # Write metadata via side-channel
    metadata = {
        'type': 'dataframe',
        'path': str(filepath.absolute())
    }

    if not _write_metadata(metadata):
        # Not in knot environment, print DataFrame normally
        print(df)

    return df
