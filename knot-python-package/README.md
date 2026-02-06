# Knot Python Package

Python support for [Knot](https://github.com/your-repo/knot) literate programming with Typst.

## Features

- **DataFrame output**: Convert pandas DataFrames to Typst tables
- **Matplotlib plots**: Render matplotlib figures in Typst documents
- **plotnine plots**: Support for Grammar of Graphics plots
- **Automatic imports**: Functions available without explicit imports in `.knot` documents

## Installation

### For Development

```bash
cd knot-python-package
pip install -e .
```

### With Optional Dependencies

```bash
# For matplotlib support
pip install -e ".[matplotlib]"

# For pandas support
pip install -e ".[pandas]"

# For plotnine support
pip install -e ".[plotnine]"

# Install all optional dependencies
pip install -e ".[all]"
```

## Usage

### In .knot Documents

The `knot` package is automatically imported in Python chunks:

```python
```{python}
import matplotlib.pyplot as plt
import pandas as pd

# DataFrames
df = pd.DataFrame({'x': [1, 2, 3], 'y': [4, 5, 6]})
typst(df)  # No need for: from knot import typst

# Matplotlib plots
plt.plot([1, 2, 3], [1, 4, 9])
plt.title("My Plot")
typst(current_plot())

# Or with explicit Figure object
fig, ax = plt.subplots()
ax.plot([1, 2, 3])
typst(fig)
```
```

### Standalone Python

If using outside of `.knot` documents:

```python
from knot import typst, current_plot

import matplotlib.pyplot as plt

plt.plot([1, 2, 3])
# Note: This won't work outside .knot as there's no side-channel
# But the API is the same
typst(current_plot())
```

## API Reference

### `typst(obj, **kwargs)`

Convert Python objects to Typst representations.

**Supported types:**
- `pandas.DataFrame` - Converted to CSV and rendered as table
- `matplotlib.figure.Figure` - Saved as image (SVG/PNG/PDF)
- `plotnine.ggplot` - Rendered via matplotlib backend

**Arguments:**
- `obj`: Object to convert
- `**kwargs`: Type-specific options
  - For plots: `width`, `height`, `dpi`, `format`
  - For DataFrames: `index` (bool, default False)

**Returns:** The original object (for chaining)

### `current_plot()`

Get the current matplotlib figure.

**Returns:** `matplotlib.figure.Figure` - The current figure from `plt.gcf()`

**Raises:**
- `RuntimeError` - If matplotlib is not installed
- `ValueError` - If no active figure or figure is empty

## Examples

### Basic DataFrame

```python
import pandas as pd
from knot import typst

df = pd.DataFrame({
    'name': ['Alice', 'Bob', 'Charlie'],
    'age': [25, 30, 35]
})
typst(df)
```

### Matplotlib Plot

```python
import matplotlib.pyplot as plt
from knot import typst, current_plot

plt.figure(figsize=(8, 6))
plt.plot([1, 2, 3, 4], [1, 4, 9, 16])
plt.xlabel('X')
plt.ylabel('Y')
plt.title('Square Function')
typst(current_plot())
```

### plotnine Plot

```python
import pandas as pd
from plotnine import ggplot, aes, geom_point, geom_line
from knot import typst

df = pd.DataFrame({'x': [1, 2, 3, 4], 'y': [1, 4, 9, 16]})
p = (
    ggplot(df, aes('x', 'y'))
    + geom_point()
    + geom_line()
)
typst(p)
```

## Environment Variables

The knot executor sets these environment variables for chunk options:

- `KNOT_FIG_WIDTH` - Figure width in inches (default: 7)
- `KNOT_FIG_HEIGHT` - Figure height in inches (default: 5)
- `KNOT_FIG_DPI` - Resolution in DPI (default: 300)
- `KNOT_FIG_FORMAT` - Output format: svg, png, pdf (default: svg)
- `KNOT_CACHE_DIR` - Directory for output files
- `KNOT_METADATA_FILE` - Side-channel metadata file (JSON)

## License

MIT
