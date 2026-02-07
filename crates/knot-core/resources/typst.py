"""
Knot - Python support for literate programming with Typst

This module provides functions to convert Python objects (DataFrames, plots)
to Typst-compatible output via side-channel communication.
"""

import os
import json
import hashlib
import pickle
import types
import importlib
import sys
import pydoc
import builtins
from pathlib import Path
from typing import Any, Optional


def _get_base_dir() -> Path:
    """Get cache directory from environment or temp."""
    cache_dir = os.environ.get('KNOT_CACHE_DIR')
    if cache_dir:
        cache_path = Path(cache_dir)
        cache_path.mkdir(parents=True, exist_ok=True)
        return cache_path

    import tempfile
    return Path(tempfile.gettempdir())


def _write_metadata(metadata: dict) -> bool:
    """Write metadata to side-channel file."""
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

# --- Session Management ---

def save_session(path):
    """Saves the global session (from __main__) including modules."""
    import sys
    try:
        import __main__
        main_dict = __main__.__dict__
        
        state = {'__knot_modules__': {}}
        for k, v in list(main_dict.items()):
            if k.startswith('__') or k in ['save_session', 'load_session', 'typst', 'current_plot']:
                continue
            
            if isinstance(v, types.ModuleType):
                state['__knot_modules__'][k] = v.__name__
                continue
                
            try:
                pickle.dumps(v)
                state[k] = v
            except:
                pass
                
        with open(path, 'wb') as f:
            pickle.dump(state, f)
        return True
    except Exception as e:
        print(f"Python Error in save_session: {e}", file=sys.stderr)
        return False

def load_session(path):
    """Restores a session into __main__."""
    import sys
    try:
        if not os.path.exists(path):
            return False
            
        import __main__
        main_dict = __main__.__dict__
            
        with open(path, 'rb') as f:
            state = pickle.load(f)
            
        modules = state.pop('__knot_modules__', {})
        for alias, name in modules.items():
            try:
                main_dict[alias] = importlib.import_module(name)
            except:
                pass
                
        main_dict.update(state)
        return True
    except Exception as e:
        print(f"Python Error in load_session: {e}", file=sys.stderr)
        return False

# --- LSP Support ---

def get_hover(topic):
    """Documentation from __main__ context."""
    try:
        import __main__
        main_dict = __main__.__dict__
        
        obj = main_dict.get(topic)
        if obj is None:
            try:
                obj = eval(topic, main_dict)
            except:
                pass
        
        if obj is not None:
            doc = pydoc.getdoc(obj)
            if not doc:
                doc = pydoc.render_doc(obj, renderer=pydoc.plaintext)
            return doc
        else:
            return pydoc.render_doc(topic, renderer=pydoc.plaintext)
    except Exception as e:
        return f"No help found for '{topic}'"

def get_completions(token):
    """Completions from __main__ context."""
    try:
        import __main__
        main_dict = __main__.__dict__
        
        if '.' in token:
            parts = token.split('.')
            base_name = parts[0]
            prefix = parts[-1]
            
            obj = main_dict.get(base_name)
            if obj is not None:
                for part in parts[1:-1]:
                    obj = getattr(obj, part, None)
                    if obj is None: break
            
            if obj is not None:
                return "\n".join([attr for attr in dir(obj) if attr.startswith(prefix) and not attr.startswith('_')])
        else:
            candidates = list(main_dict.keys()) + dir(builtins)
            return "\n".join([c for c in candidates if c.startswith(token) and not c.startswith('_')])
    except:
        return ""
    return ""
