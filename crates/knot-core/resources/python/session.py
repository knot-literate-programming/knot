"""Knot Python Session Management"""

import os
import sys
import pickle
import types
import importlib


def save_session(path):
    """Saves the global session (from __main__) including modules."""
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
