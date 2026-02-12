# Consolidated Example Project

This is a comprehensive demonstration of Knot's capabilities for literate programming with R and Python.

## Project Structure

```
consolidated/
├── knot.toml              # Project configuration
├── main.knot              # Main document with global title
├── r-analysis.knot        # R-focused chapter (ggplot2)
├── python-analysis.knot   # Python-focused chapter (matplotlib OOP)
├── mixed-analysis.knot    # Mixed R+Python with data sharing
└── lib/
    └── knot.typ          # Helper functions
```

## What This Example Demonstrates

### 1. Multi-File Project Structure
- Main entry point with chapter injection
- Three separate analysis files
- Organized, modular documentation

### 2. R Analysis (`r-analysis.knot`)
- **ggplot2 visualizations** (primary use case)
- Data preparation and summary statistics
- Line plots, box plots, scatter plots with regression
- Various chunk options:
  - `layout: horizontal` - side-by-side code and output
  - `layout: vertical` - stacked presentation
  - `show: code` - hide results, show only code
  - `show: output` - hide code, show only results
  - Custom colors and borders

### 3. Python Analysis (`python-analysis.knot`)
- **Matplotlib OOP style** (`fig, ax = plt.subplots()`)
- Single plots and multi-panel figures
- Statistical visualizations with regression
- Different styling options per chunk

### 4. Language Integration (`mixed-analysis.knot`)
- **R and Python in the same file**
- **Data interchange**: R writes CSV → Python reads and analyzes
- Round-trip demonstration: R → CSV → Python → R
- Complex multi-panel matplotlib figures
- Real-world research workflow

### 5. Chunk Customization
Throughout the examples, you'll see different chunk options:
- `show: code/output/both` - Show/hide code or results
- `eval: true/false` - Execute/skip chunk
- `layout` - Horizontal or vertical
- `fig-width`, `fig-height` - Figure dimensions
- `code-background`, `output-background` - Color customization
- `code-stroke`, `output-stroke` - Border styling
- `gutter`, `code-inset`, `output-inset` - Spacing adjustments

## How to Use

### Compile the Document

```bash
# From the consolidated/ directory
knot compile main.knot
```

This generates `.main.typ` with all chapters included.

### Generate PDF

```bash
typst compile .main.typ output.pdf
```

### Watch Mode (Live Preview)

```bash
knot watch
```

Edit any `.knot` file and see the PDF update automatically!

## Requirements

- **Knot** CLI tools
- **Typst** for PDF generation
- **R** with packages:
  - ggplot2
  - dplyr
- **Python** with packages:
  - pandas
  - numpy
  - matplotlib
  - scipy

Install R packages:
```r
install.packages(c("ggplot2", "dplyr"))
```

Install Python packages:
```bash
pip install pandas numpy matplotlib scipy
```

## Output

The compiled PDF contains:
- Professional title page
- Three complete chapters with 10+ figures
- Mixed R and Python code with rich output
- Publication-quality ggplot2 and matplotlib graphics
- Data tables and statistical summaries

## Learning Points

This example is ideal for learning:
- How to structure multi-file Knot projects
- ggplot2 best practices for R graphics
- Matplotlib OOP patterns for Python
- Data interchange between languages
- Chunk option customization for different presentation needs

---

**Ready to explore?** Open the `.knot` files in VS Code and start experimenting! 🎨
