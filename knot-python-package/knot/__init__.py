"""
Knot - Python support for literate programming with Typst

Main functions:
- typst(obj): Convert Python objects to Typst output
- current_plot(): Get the current matplotlib figure

Usage in .knot documents:
    # knot is automatically imported with: from knot import *

    import matplotlib.pyplot as plt
    plt.plot([1, 2, 3])
    typst(current_plot())
"""

from .core import typst, current_plot

__version__ = "0.1.0"

__all__ = [
    'typst',
    'current_plot',
]
