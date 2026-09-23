"""Knot Python Session Management"""

import os
import sys
import pickle
import types
import importlib
import io


class _KnotSnapshotPickler(pickle.Pickler):
    def persistent_id(self, obj):
        # Standard pickle cannot recreate user definitions in a fresh process,
        # including references nested in lists or class instances. Replay instead.
        if (getattr(obj, "__module__", None) == "__main__"
                or getattr(type(obj), "__module__", None) == "__main__"):
            raise pickle.PicklingError("Session contains a user-defined object")
        return None


def save_session(path, excluded=()):
    """Save selected global bindings without changing the live session."""
    try:
        import __main__
        main_dict = __main__.__dict__

        state = {'__knot_modules__': {}, '__knot_cwd__': os.getcwd()}
        reusable = True
        excluded = set(excluded)
        initial = main_dict.get('_knot_initial_bindings', {})
        for k, v in list(main_dict.items()):
            if k in excluded:
                continue
            if k.startswith('__') or k.startswith('_knot_') or (k in initial and v is initial[k]):
                continue

            if isinstance(v, types.ModuleType):
                state['__knot_modules__'][k] = v.__name__
                continue

            try:
                _KnotSnapshotPickler(io.BytesIO()).dump(v)
                state[k] = v
            except Exception:
                reusable = False

        replay_path = os.path.splitext(path)[0] + '.replay'
        if reusable:
            if os.path.exists(replay_path):
                os.remove(replay_path)
        else:
            with open(replay_path, 'w') as marker:
                marker.write('Session requires replay: not all objects are serializable.')

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

        os.chdir(state.pop('__knot_cwd__'))
        modules = state.pop('__knot_modules__', {})
        for alias, name in modules.items():
            main_dict[alias] = importlib.import_module(name)

        main_dict.update(state)
        return True
    except Exception as e:
        print(f"Python Error in load_session: {e}", file=sys.stderr)
        return False
