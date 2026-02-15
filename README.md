# Knot

**knot is not knitr** — A modern literate programming system for Typst

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![GitHub Release](https://img.shields.io/github/v/release/knot-literate-programming/knot)](https://github.com/knot-literate-programming/knot/releases)

---

## What is Knot?

Knot brings **executable R and Python code** to [Typst](https://typst.app/) documents. Write your analysis, visualizations, and text together in one place — Knot executes the code, caches results intelligently, and generates beautiful PDFs.

**Think:** RMarkdown or Jupyter, but with Typst's modern typesetting instead of Markdown/LaTeX.

### Why Knot?

- 🚀 **Modern stack**: Typst + Rust (fast, reliable, no LaTeX hell)
- 🔬 **Reproducible**: Deterministic execution, intelligent caching
- 🎨 **Beautiful output**: Professional PDFs with Typst's power
- 💻 **IDE integration**: VS Code extension with hover, completion, diagnostics
- 📦 **Multi-file projects**: Organize large documents with `knot.toml`
- 🐍🔵 **R + Python**: Switch between languages seamlessly

---

## Quick Start

### Installation

**Download from [GitHub Releases](https://github.com/knot-literate-programming/knot/releases):**

1. **CLI tools** (`knot`, `knot-lsp`) for your platform (macOS/Linux/Windows)
2. **VS Code extension** (`knot-X.Y.Z.vsix`)

```bash
# Install CLI (example for macOS/Linux)
tar -xzf knot-*-{platform}.tar.gz
mv knot knot-lsp ~/.local/bin/  # or /usr/local/bin

# Install VS Code extension
code --install-extension knot-0.1.0.vsix
```

**Verify installation:**
```bash
knot --version
```

### Your First Document

Create a new project:
```bash
knot init my-analysis
cd my-analysis
```

Edit `main.knot`:
```typst
#import "lib/knot.typ": *

= My First Analysis

Here's some data analysis with R:

```{r}
x <- 1:10
mean(x)
```

And a plot with Python:

```{python}
import matplotlib.pyplot as plt
import numpy as np

x = np.linspace(0, 2*np.pi, 100)
plt.plot(x, np.sin(x))
typst(plt.gcf())  # Send plot to Typst
```
```

Compile to PDF:
```bash
knot compile main.knot
typst compile .main.typ output.pdf
```

**Or use watch mode** for live preview:
```bash
knot watch
```

👉 **[Full tutorial in QUICKSTART.md](QUICKSTART.md)**

---

## Features

### Execution
- ✅ R and Python with persistent sessions
- ✅ Intelligent caching (SHA256-based invalidation)
- ✅ Rich output: DataFrames → tables, plots → SVG/PNG

### Project Management
- ✅ Multi-file documents via `knot.toml` includes
- ✅ Configurable chunk defaults (graphics, styling, behavior)
- ✅ Watch mode with live PDF preview

### IDE Support (VS Code)
- ✅ Syntax highlighting (Typst + embedded R/Python)
- ✅ Hover information (variables, functions, chunks)
- ✅ Code completion (chunk options, R/Python)
- ✅ Diagnostics (parsing errors, invalid options)
- ✅ Document symbols (chunk navigation)

### Chunk Customization
- ✅ 12+ presentation options (layout, colors, borders, spacing)
- ✅ Graphics options (size, format, DPI)
- ✅ Execution control (eval, show, cache)

---

## Documentation

- **[QUICKSTART.md](QUICKSTART.md)** — Step-by-step tutorial
- **[Example Project](examples/)** — Complete multi-file example with R, Python, graphics *(coming soon)*
- **[Dev Plans](docs/dev-plans/)** — Architecture and roadmap

---

## Project Status

**Current version:** v0.1.6 (Early Testing)

Knot is in active development and ready for **early adopters and testers**. The core features work well, but expect rough edges and breaking changes before v1.0.

**What works today:**
- Core compilation pipeline (R, Python, caching)
- VS Code extension with LSP
- Multi-file projects
- Graphics and rich output
- Watch mode

**What's coming:**
- Structured error handling (precise line numbers for R/Python errors)
- Go to Definition
- Hybrid formatting (Air for R, Ruff for Python)
- User documentation and tutorials

---

## Contributing

We're looking for **early testers** and **contributors**!

### Testing & Feedback

Found a bug or have a suggestion?
- 🐛 [Open an issue](https://github.com/knot-literate-programming/knot/issues)
- 💬 Share your experience (what works, what doesn't)
- 📝 Try the example project and report issues

### Contributing Code

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for development setup and guidelines.

**Good first issues:** [Link to issues](https://github.com/knot-literate-programming/knot/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)

---

## Architecture

- **`knot-core`** — Parser, executor, cache engine
- **`knot-cli`** — Command-line interface (compile, watch, init)
- **`knot-lsp`** — Language Server Protocol implementation
- **`editors/vscode`** — VS Code extension

Built with Rust for performance and reliability.

---

## License

MIT License — See [LICENSE](LICENSE) for details.

---

## Acknowledgments

Inspired by:
- [knitr](https://yihui.org/knitr/) — The OG literate programming for R
- [Quarto](https://quarto.org/) — Modern scientific publishing
- [Typst](https://typst.app/) — The future of typesetting

Developed with the assistance of [Claude](https://claude.ai) (Anthropic) as a pair-programming tool for code generation and architecture design.

---

**Ready to try it?** → [QUICKSTART.md](QUICKSTART.md)
