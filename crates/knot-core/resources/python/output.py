"""Knot Python Output Formatting"""

import os
import hashlib
from functools import singledispatch
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


@singledispatch
def typst(obj: Any, **kwargs) -> Any:
    """Convert Python objects to Typst representations.

    Dispatches on the type of *obj*. Built-in handlers are registered lazily
    (on first call) for ``matplotlib.figure.Figure``, ``plotnine.ggplot``,
    and ``pandas.DataFrame`` when those libraries are available.

    Users can register handlers for their own types without modifying Knot::

        typst.register(MyClass)(lambda obj, **kwargs: ...)
    """
    _register_optional_handlers()
    impl = typst.dispatch(type(obj))
    if impl is typst.dispatch(object):
        print(obj)
        return obj
    return impl(obj, **kwargs)


_optional_handlers_registered = False


def _register_optional_handlers() -> None:
    """Register built-in handlers for optional dependencies (run once)."""
    global _optional_handlers_registered
    if _optional_handlers_registered:
        return
    _optional_handlers_registered = True

    try:
        import matplotlib.figure
        typst.register(matplotlib.figure.Figure)(
            lambda fig, **kw: _typst_matplotlib(fig, **kw)
        )
    except ImportError:
        pass

    try:
        from plotnine import ggplot
        typst.register(ggplot)(
            lambda gg, **kw: _typst_plotnine(gg, **kw)
        )
    except ImportError:
        pass

    try:
        import pandas as pd
        typst.register(pd.DataFrame)(
            lambda df, **kw: _typst_dataframe(df, **kw)
        )
    except ImportError:
        pass


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
