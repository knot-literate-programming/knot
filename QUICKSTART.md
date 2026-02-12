# Knot Quick Start Guide

Get from zero to your first Knot document in 10 minutes.

---

## Prerequisites

Before starting, you need:
- **Typst CLI** — [Install from typst.app](https://github.com/typst/typst#installation)
- **R** (optional) — [Download from r-project.org](https://www.r-project.org/)
- **Python** (optional) — [Download from python.org](https://www.python.org/) or use system Python
- **VS Code** (recommended) — [Download from code.visualstudio.com](https://code.visualstudio.com/)

---

## Step 1: Install Knot

### Download the release

Go to [GitHub Releases](https://github.com/knot-literate-programming/knot/releases) and download:

1. **CLI tools** for your platform:
   - macOS: `knot-*-aarch64-apple-darwin.tar.gz` (Apple Silicon) or `x86_64-apple-darwin.tar.gz` (Intel)
   - Linux: `knot-*-x86_64-unknown-linux-gnu.tar.gz`
   - Windows: `knot-*-x86_64-pc-windows-msvc.zip`

2. **VS Code extension**: `knot-*.vsix`

### Install CLI tools

**macOS/Linux:**
```bash
# Extract archive
tar -xzf knot-*-{your-platform}.tar.gz

# Move to PATH (choose one location)
sudo mv knot knot-lsp /usr/local/bin/
# Or for user-only install:
mkdir -p ~/.local/bin
mv knot knot-lsp ~/.local/bin/
export PATH="$HOME/.local/bin:$PATH"  # Add to ~/.bashrc or ~/.zshrc
```

**Windows:**
```powershell
# Extract zip
Expand-Archive knot-*.zip

# Add to PATH via System Properties > Environment Variables
# Or run from current directory: .\knot.exe
```

**Verify:**
```bash
knot --version
knot-lsp --version
```

### Install VS Code extension

```bash
code --install-extension knot-*.vsix
```

Or via VS Code UI:
1. Open VS Code
2. Go to Extensions (`Ctrl+Shift+X` / `Cmd+Shift+X`)
3. Click `⋯` (more actions)
4. Select `Install from VSIX...`
5. Choose the downloaded `.vsix` file

---

## Step 2: Create Your First Project

```bash
knot init my-first-document
cd my-first-document
```

This creates:
```
my-first-document/
├── knot.toml          # Project configuration
├── main.knot          # Your document
└── lib/
    └── knot.typ       # Knot helper functions (imported automatically)
```

**Open in VS Code:**
```bash
code .
```

---

## Step 3: Understanding the Structure

### `knot.toml` — Project configuration

```toml
[document]
main = "main.knot"
# includes = ["chapter1.knot", "chapter2.knot"]  # For multi-file projects

[helpers]
typst = "lib/knot.typ"

[defaults]
# Global chunk options (all commented by default)
# eval = true
# show = "both"
# fig-width = 7.0
# layout = "horizontal"
```

### `main.knot` — Your document

```typst
#import "lib/knot.typ": *

// Codly configuration (syntax highlighting)
#import "@preview/codly:1.3.0": *
#show: codly-init
#import "@preview/codly-languages:0.1.10": *
#codly(languages: codly-languages)

// Figure numbering
#set figure(numbering: "1.")

= My Document

This is a Knot document with embedded R/Python code.
```

---

## Step 4: Write Your First Chunks

Edit `main.knot` and add some code:

### R Example

```typst
= Data Analysis with R

Let's analyze the famous iris dataset:

```{r}
#| show: "both"

# Load data
data(iris)

# Summary statistics
summary(iris$Sepal.Length)

# Create a simple plot
library(ggplot2)
gg <- ggplot(iris, aes(x = Sepal.Length, y = Sepal.Width, color = Species)) +
  geom_point(size = 3) +
  theme_minimal()

typst(gg, width = 6, height = 4)  # Send plot to Typst
```

The mean sepal length is `{r} mean(iris$Sepal.Length)` cm.
```

### Python Example

```typst
= Machine Learning with Python

```{python}
#| show: "both"

import pandas as pd
import matplotlib.pyplot as plt
import numpy as np

# Generate data
x = np.linspace(0, 10, 100)
y = np.sin(x) + np.random.normal(0, 0.1, 100)

# Plot
fig, ax = plt.subplots(figsize=(6, 4))
ax.scatter(x, y, alpha=0.5)
ax.plot(x, np.sin(x), 'r-', linewidth=2, label='True function')
ax.legend()
ax.set_title('Noisy Sine Wave')

typst(fig)  # Send plot to Typst
```

The data has `{python} len(x)` points.
```

---

## Step 5: Compile Your Document

### One-shot compilation

```bash
knot compile main.knot
```

This generates `.main.typ` (a pure Typst file with executed results).

Compile to PDF:
```bash
typst compile .main.typ output.pdf
```

Open `output.pdf` to see your document!

### Watch mode (recommended)

```bash
knot watch
```

This:
1. Compiles `main.knot` → `.main.typ` on changes
2. Launches `typst watch` for live PDF preview
3. Watches all `.knot` files and `knot.toml`

**Edit `main.knot` in VS Code** → Save → PDF updates automatically! 🎉

Stop with `Ctrl+C` or use the "Stop Preview" button in VS Code.

---

## Step 6: Chunk Options

Control how chunks behave and appear:

### Execution Options

```typst
```{r}
#| eval: true      # Execute this chunk (default: true)
#| show: "both"    # "both", "code", "output", or "none"
#| cache: true     # Cache results (default: true)

x <- 1:10
mean(x)
```
```

### Graphics Options

```typst
```{python}
#| fig-width: 8       # Figure width in inches
#| fig-height: 5      # Figure height in inches
#| dpi: 300           # Resolution for raster graphics
#| fig-format: "svg"  # Format: "svg" or "png"

import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 4, 9])
typst(plt.gcf())
```
```

### Presentation Options

```typst
```{r}
#| layout: "vertical"          # "horizontal" or "vertical"
#| gutter: "1em"               # Space between code and output
#| code-background: "#f5f5f5"  # Code block background color
#| output-background: "#e8f4f8" # Output block background color

hist(rnorm(1000))
```
```

**See all options** in `knot.toml` `[defaults]` section (all options are documented there).

---

## Step 7: Multi-File Projects

For larger documents, split into multiple files:

**Structure:**
```
my-project/
├── knot.toml
├── main.knot
├── chapter1.knot
├── chapter2.knot
└── lib/
    └── knot.typ
```

**Update `knot.toml`:**
```toml
[document]
main = "main.knot"
includes = ["chapter1.knot", "chapter2.knot"]
```

**In `main.knot`:**
```typst
#import "lib/knot.typ": *

= My Book

/* KNOT-INJECT-CHAPTERS */

= Conclusion

...
```

The `/* KNOT-INJECT-CHAPTERS */` placeholder will be replaced with the compiled content of all included files.

**Compile:**
```bash
knot compile main.knot  # Automatically includes chapter1.knot and chapter2.knot
```

---

## Step 8: Using the VS Code Extension

Open any `.knot` file in VS Code and you get:

### Features

1. **Syntax highlighting**
   - Typst markup
   - Embedded R/Python code
   - Chunk options

2. **Hover** (`Ctrl`/`Cmd` + hover over symbol)
   - R/Python functions → documentation
   - Variables → type/value info
   - Chunk headers → chunk info

3. **Completion** (`Ctrl+Space`)
   - Chunk option names (type `#|` in a chunk)
   - R/Python symbols
   - Typst syntax

4. **Diagnostics** (red squiggles)
   - Malformed chunks
   - Invalid chunk options
   - Parsing errors

5. **Commands** (Cmd/Ctrl+Shift+P)
   - `Knot: Open Preview` — Start watch mode
   - `Knot: Clean Project` — Clear cache
   - `Knot: Stop Preview` — Stop watch mode

### Toolbar Buttons

When a `.knot` file is open:
- 👁️ **Open Preview** — Quick watch mode
- 🗑️ **Clean Project** — Clear cache
- ⏹️ **Stop Preview** — Stop watch

---

## Step 9: Project Defaults

Set global defaults in `knot.toml` to avoid repeating chunk options:

```toml
[defaults]
# All chunks will inherit these unless overridden
eval = true
show = "output"       # "both", "code", "output", or "none"
cache = true

# Graphics
fig-width = 8.0
fig-height = 5.0
dpi = 300
fig-format = "svg"

# Presentation
layout = "horizontal"
gutter = "1em"
code-background = "#f8f9fa"
output-background = "#e9ecef"
```

Override in specific chunks:
```typst
```{r}
#| show: "both"    # Show code for THIS chunk only
x <- 1:10
```
```

---

## Step 10: Tips & Tricks

### Cache Management

Knot caches chunk results based on:
- Chunk code content (SHA256)
- Chunk options
- Dependencies (previous chunks)

**Clear cache:**
```bash
knot clean-project
# Or in VS Code: Cmd+Shift+P → "Knot: Clean Project"
```

**When cache updates:**
- ✅ Code changes → chunk re-executes
- ✅ Option changes → chunk re-executes
- ✅ Dependency changes → dependent chunks re-execute
- ❌ External file changes → manual clean needed

### Inline Expressions

Quick computations inline:

```typst
The mean is `{r} mean(x)` and the standard deviation is `{r} sd(x)`.

Today's date: `{python} import datetime; datetime.date.today()`
```

### R Packages

Install once, use in all projects:
```r
install.packages(c("tidyverse", "ggplot2", "knitr"))
```

Use in chunks:
```typst
```{r}
library(tidyverse)
iris %>% filter(Species == "setosa") %>% summary()
```
```

### Python Packages

```bash
pip install pandas matplotlib numpy seaborn plotnine
```

Use in chunks:
```typst
```{python}
import pandas as pd
import seaborn as sns
```
```

---

## Troubleshooting

### "Command not found: knot"

**Fix:** Add Knot to your PATH.
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### "R/Python not found"

**Fix:** Install R/Python and ensure they're in PATH:
```bash
which R
which python3
```

### Plots don't appear

**For R:**
- Use `typst(gg)` for ggplot2 plots
- Use `current_plot()` for base R plots (limited support)

**For Python:**
- Use `typst(fig)` after creating a matplotlib figure
- Or `typst(plt.gcf())` to get current figure

### Cache issues

If results are stale:
```bash
knot clean-project
knot compile main.knot
```

---

## Next Steps

- 📖 **Read the [example project](examples/)** — A complete multi-file document
- 🔧 **Configure [chunk defaults](knot.toml)** — Set project-wide options
- 🎨 **Explore [presentation options](docs/dev-plans/chunk-customization.md)** — Customize chunk appearance
- 🐛 **Report issues** — [GitHub Issues](https://github.com/knot-literate-programming/knot/issues)
- 💬 **Give feedback** — What works? What doesn't?

---

**Happy Knotting!** 🧶✨
