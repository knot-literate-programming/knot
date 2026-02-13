"""Knot Python Helpers - Internal utility functions"""

import os
import json
from pathlib import Path

# Internal state
_knot_results = []
_knot_warnings = []
_knot_error = None


def _knot_clear_state():
    global _knot_results, _knot_warnings, _knot_error
    _knot_results = []
    _knot_warnings = []
    _knot_error = None


def _knot_add_warning(message, line=None):
    """Capture a warning into the side-channel state."""
    _knot_warnings.append({
        'message': str(message),
        'line': line
    })


def _get_base_dir() -> Path:
    """Get cache directory from environment or temp."""
    cache_dir = os.environ.get('KNOT_CACHE_DIR')
    if cache_dir:
        cache_path = Path(cache_dir)
        cache_path.mkdir(parents=True, exist_ok=True)
        return cache_path

    import tempfile
    return Path(tempfile.gettempdir())


def _write_metadata(metadata, type='result') -> bool:
    """Write structured metadata to side-channel file.

    Uses auto_unbox-equivalent semantics: all scalar fields are plain JSON
    values, not arrays. Lists that must stay arrays (traceback) are passed
    as Python lists directly.
    """
    metadata_file = os.environ.get('KNOT_METADATA_FILE')
    if not metadata_file:
        return False

    global _knot_results, _knot_error

    # Update internal state
    if type == 'result' and metadata is not None:
        _knot_results.append(metadata)
    elif type == 'error':
        _knot_error = metadata

    # Prepare full structured metadata object (mirrors KnotMetadata in Rust)
    data = {
        'results': _knot_results,
        'warnings': _knot_warnings,
    }
    if _knot_error is not None:
        data['error'] = _knot_error

    try:
        with open(metadata_file, 'w') as f:
            json.dump(data, f)
    except Exception:
        # Fallback: write minimal valid metadata so Rust doesn't hang
        with open(metadata_file, 'w') as f:
            f.write('{"results": [], "warnings": []}')

    return True
