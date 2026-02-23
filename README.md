# Knot

**knot is not knitr** — A modern literate programming system for [Typst](https://typst.app/).

> [!IMPORTANT]
> **Work in Progress**: Knot is currently under intensive development. Many features are in a "preview" state and may change rapidly.

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80+-orange.svg)](https://www.rust-lang.org)
[![GitHub Release](https://img.shields.io/github/v/release/knot-literate-programming/knot)](https://github.com/knot-literate-programming/knot/releases)

---

## What is Knot?

Knot brings **executable R and Python code** directly into your Typst documents. It allows you to weave analysis, visualizations, and text into a single source, providing a modern, fast, and reproducible alternative to RMarkdown or Jupyter, powered by the performance of Rust and the beauty of Typst.

### Key Features

- **Blazing Fast**: Built with Rust, featuring a three-pass compilation pipeline for progressive updates.
- **Reproducible**: Intelligent SHA256-based caching with sequential invalidation.
- **Polyglot**: Seamlessly switch between R and Python in the same document.
- **Rich Output**: Automatic conversion of DataFrames to Typst tables and plots (Matplotlib, ggplot2) to SVG/PNG.
- **First-class IDE support**: A dedicated VS Code extension providing hover docs, completion, and live diagnostics.
- **Sync Mapping**: High-fidelity PDF ↔ Source synchronization (PDF to Source is line-perfect; Source to PDF is currently WIP).

---

## Quick Start

### 1. Installation

**Using pre-compiled binaries:**
Download the CLI tools (`knot`, `knot-lsp`) and the VS Code extension (`.vsix`) for your platform from the [latest releases](https://github.com/knot-literate-programming/knot/releases).

**Using Cargo:**
```bash
cargo install --git https://github.com/knot-literate-programming/knot.git knot-cli
```

### 2. Create a Project
```bash
knot init my-project
cd my-project
code . # Open in VS Code
```

### 3. Write and Compile
Edit `main.knot`:

~~~typst
#import "lib/knot.typ": *

= Analysis

```{r}
#| show: "both"
x <- rnorm(100)
hist(x, col="steelblue")
typst(current_plot())
```

The average value is `{python} import numpy; print(numpy.mean(numpy.random.randn(100)))`.
~~~

Compile it:
```bash
knot watch # Start live preview mode
```

---

## Project Structure

- **`knot-core`**: The engine (parser, executors, cache).
- **`knot-lsp`**: Language Server for IDE features.
- **`knot-cli`**: Command-line interface.
- **`editors/vscode`**: VS Code extension source.

---

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

---

## Citing Knot

If you use Knot for your research, please cite it as follows:

```bibtex
@software{Klutchnikoff_Knot_A_modern_2026,
  author = {Klutchnikoff, Nicolas},
  month = feb,
  title = {{Knot: A modern literate programming system for Typst}},
  url = {https://github.com/knot-literate-programming/knot},
  version = {0.2.5},
  year = {2026}
}
```

See [CITATION.cff](CITATION.cff) for full metadata.

---

## License

Knot is licensed under the [Apache License, Version 2.0](LICENSE). This matches the license used by Typst.

---

## Acknowledgments

Inspired by [knitr](https://yihui.org/knitr/), [Quarto](https://quarto.org/), and [Typst](https://typst.app/).
Developed with the assistance of [Claude](https://claude.ai) as a pair-programming partner.
