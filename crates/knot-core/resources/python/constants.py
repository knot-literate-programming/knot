"""Knot Python Constant Objects Management"""

import os
import pickle
import hashlib


def hash_object(object_name):
    """Compute hash of an object for constant caching."""
    import __main__
    main_dict = __main__.__dict__

    if object_name not in main_dict:
        return "NONE"

    obj = main_dict[object_name]

    # Try xxhash first (faster), fallback to sha256
    try:
        import xxhash
        h = xxhash.xxh64(pickle.dumps(obj)).hexdigest()
    except ImportError:
        # Fallback to hashlib if xxhash is not available
        h = hashlib.sha256(pickle.dumps(obj)).hexdigest()

    return h


def save_constant(object_name, path):
    """Save a constant object to a file."""
    import __main__
    main_dict = __main__.__dict__

    if object_name in main_dict:
        with open(path, 'wb') as f:
            pickle.dump(main_dict[object_name], f)
        return True
    return False


def load_constant(object_name, path):
    """Load a constant object from a file."""
    import __main__
    main_dict = __main__.__dict__

    if os.path.exists(path):
        with open(path, 'rb') as f:
            main_dict[object_name] = pickle.load(f)
        return True
    return False


def remove_from_env(object_name):
    """Remove an object from the global environment."""
    import __main__
    main_dict = __main__.__dict__

    if object_name in main_dict:
        del main_dict[object_name]
        return True
    return False
