"""Knot Python Output Formatting"""

import os
import hashlib
from typing import Any, Optional


def current_plot():
    """Get the current matplotlib figure."""
    try:
        import matplotlib.pyplot as plt
    except ImportError:
        raise RuntimeError("matplotlib is required for current_plot().")

    fig = plt.gcf()
    if not fig.get_axes():
        raise ValueError("Current figure is empty.")
    return fig


def typst(obj: Any, **kwargs) -> Any:
    """Convert Python objects to Typst representations."""
    try:
        import matplotlib.figure
        if isinstance(obj, matplotlib.figure.Figure):
            return _typst_matplotlib(obj, **kwargs)
    except ImportError:
        pass

    try:
        from plotnine import ggplot
        if isinstance(obj, ggplot):
            return _typst_plotnine(obj, **kwargs)
    except ImportError:
        pass

    try:
        import pandas as pd
        if isinstance(obj, pd.DataFrame):
            return _typst_dataframe(obj, **kwargs)
    except ImportError:
        pass

    print(obj)
    return obj


def _typst_matplotlib(fig, width=None, height=None, dpi=None, format=None):
    import io
    import matplotlib.pyplot as plt

    width = width or float(os.environ.get('KNOT_FIG_WIDTH', '7'))
    height = height or float(os.environ.get('KNOT_FIG_HEIGHT', '5'))
    dpi = dpi or int(os.environ.get('KNOT_FIG_DPI', '300'))
    format = format or os.environ.get('KNOT_FIG_FORMAT', 'svg')

    fig.set_size_inches(width, height)
    buf = io.BytesIO()
    fig.savefig(buf, format='svg', bbox_inches='tight')
    fig_hash = hashlib.sha256(buf.getvalue()).hexdigest()[:16]

    filename = f"plot_{fig_hash}.{format}"
    filepath = _get_base_dir() / filename
    fig.savefig(filepath, format=format, dpi=dpi, bbox_inches='tight')

    metadata = {'type': 'plot', 'path': str(filepath.absolute()), 'format': format}
    if not _write_metadata(metadata):
        plt.show()
    return fig


def _typst_plotnine(gg, **kwargs):
    fig = gg.draw()
    _typst_matplotlib(fig, **kwargs)
    return gg


def _typst_dataframe(df, index=False, **kwargs):
    df_string = df.to_string()
    df_hash = hashlib.sha256(df_string.encode()).hexdigest()[:16]
    filename = f"dataframe_{df_hash}.csv"
    filepath = _get_base_dir() / filename
    df.to_csv(filepath, index=index)
    metadata = {'type': 'dataframe', 'path': str(filepath.absolute())}
    if not _write_metadata(metadata):
        print(df)
    return df
