"""Knot Python Helpers - Internal utility functions"""

import os
import json
from pathlib import Path


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
