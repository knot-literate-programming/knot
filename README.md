# Knot

**knot is not knitr** — A modern literate programming system for Typst

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

## Overview

Knot is a literate programming tool that brings executable code to [Typst](https://typst.app/) documents. Write R and Python code directly in your Typst documents and let Knot execute it, cache results intelligently, and generate beautiful PDFs.

### Philosophy

- **Typst as the ONLY documentation language** (no Markdown intermediate)
- **Strictly linear and deterministic execution**
- **Maximum performance** (Rust + Typst vs Pandoc + LaTeX)
- **Guaranteed reproducibility**
- **Intelligent caching** with SHA256-based invalidation cascading
- **Multi-file project orchestration** via `knot.toml`

## Current Status

**v0.1.0-alpha** — Core architecture complete

✅ **Completed:**
- **Polyglot execution**: Persistent R and Python processes
- **Side-channel communication**: Robust metadata transfer for all languages
- **Rich output (R & Python)**:
  - DataFrames → Typst tables via `typst(df)`
  - Plots (ggplot2, matplotlib, plotnine) → SVG/PNG via `typst(gg/fig)`
- **Project Management**: Full orchestration with `knot.toml` and chapter injection
- **Chained Caching**: SHA256-based system with sequential invalidation
- **Watch Mode**: Automatic rebuilds with parallel Typst watch
- **LSP Support**: Diagnostics, hover, and completion via `knot-lsp`

## Document Syntax

### Code Chunks

````typst
```{r setup}
#| eval: true
#| echo: true
library(tidyverse)
data <- iris
```

```{python analysis}
import pandas as pd
from knot import typst
df = pd.DataFrame({"x": [1, 2], "y": [3, 4]})
typst(df)
```
````

### Inline Expressions

Embed code directly in your text:

- `` `{r} mean(data$Sepal.Length)` ``
- `` `{python} len(df)` ``

## Roadmap

- [x] **Phase 1-4:** R & Python execution, rich output, caching, graphics
- [x] **Phase 5:** Multi-file project orchestration and watch mode
- [ ] **Phase 6:** Typst package publication (@preview/knot)
- [ ] **Phase 7:** LilyPond support for music notation
- [ ] **Phase 8:** Performance optimizations (Parallel compilation)
- [ ] **Phase 9:** v1.0 release

---

**"knot is not knitr"** — Literate programming for the Typst era

See [knot-project-reference.txt](knot-project-reference.txt) and [DEVLOG.md](DEVLOG.md) for detailed development history.

## Comparison

| Tool    | Input       | Backend | Speed     | Cache         | Control    |
|---------|-------------|---------|-----------|---------------|------------|
| knitr   | R Markdown  | LaTeX   | ⭐⭐      | ⭐⭐⭐        | ⭐⭐       |
| Quarto  | Markdown    | Pandoc  | ⭐⭐      | ⭐⭐⭐        | ⭐⭐       |
| Jupyter | Notebook    | Kernel  | ⭐⭐⭐    | ❌            | ⭐         |
| **Knot** | **Typst**   | **Typst** | **⭐⭐⭐⭐⭐** | **⭐⭐⭐⭐⭐** | **⭐⭐⭐⭐⭐** |

## Contributing

Contributions are welcome! Please see [DEVLOG.md](DEVLOG.md) for development history and current status.

## License

MIT License — see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [knitr](https://yihui.org/knitr/) and [Quarto](https://quarto.org/)
- Built for [Typst](https://typst.app/), the modern typesetting system

---

**"knot is not knitr"** — Literate programming for the Typst era
