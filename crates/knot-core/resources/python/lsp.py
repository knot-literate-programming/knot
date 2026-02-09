"""Knot Python LSP Support"""

import pydoc
import builtins


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
