# Knot

**knot is not knitr** — A modern literate programming system for Typst

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)

## Overview

Knot is a literate programming tool that brings executable code to [Typst](https://typst.app/) documents. Write R code (with Python and LilyPond support planned) directly in your Typst documents and let Knot execute it, cache results intelligently, and generate beautiful PDFs.

### Philosophy

- **Typst as the ONLY documentation language** (no Markdown intermediate)
- **Strictly linear and deterministic execution**
- **Maximum performance** (Rust + Typst vs Pandoc + LaTeX)
- **Guaranteed reproducibility**
- **Intelligent caching** with SHA256-based invalidation cascading

## Current Status

**v0.1.0-alpha** — Phases 1-4 complete

✅ **Completed:**
- R code execution with persistent R process
- Chunk parsing with `#|` options (Quarto-style)
- SHA256 chained caching system with automatic invalidation
- File dependency tracking (`#| depends:`)
- Cross-referencing chunks with `@chunk-name`
- LSP-ready parser with position tracking
- **R package (`knot.r.package`) for rich output**
  - DataFrames → Typst tables via `typst(df)`
  - ggplot2 plots → SVG/PNG images via `typst(gg)`
- **Graphics support with explicit control**
  - Configurable dimensions, resolution, and formats
  - Full cache integration for plots

🎯 **Next Steps:**
- Phase 5: Typst package publication (@preview/knot)
- Phase 6: Inline R expressions, watch mode

## Quick Start

### Installation

**1. Install Knot CLI:**
```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/knot.git
cd knot

# Build the project
cargo build --release

# Install (optional)
cargo install --path crates/knot-cli
```

**2. Install R package (for DataFrames and plots):**
```bash
cd knot-r-package
R CMD INSTALL .
```

**3. Install required R packages:**
```r
install.packages(c("ggplot2", "digest"))
```

### Prerequisites

- **Rust** 1.70+ ([rustup](https://rustup.rs/))
- **Typst** ([installation](https://github.com/typst/typst))
- **R** (for R code execution)
- **typstyle** (optional, for formatting): `cargo install typstyle`

### Usage

**Create a new Knot document:**
```bash
knot init my-document.knot
```

**Compile to PDF:**
```bash
knot compile my-document.knot
```

**On subsequent runs, cached chunks are reused:**
```
📄 Compiling "my-document.knot"...
✓ Parsed 3 chunk(s)
🔧 Processing 3 code chunks...
  ✓ setup [cached]
  ✓ analysis [cached]
  ✓ plot [executing]
```

## Document Syntax

Knot documents use standard Typst syntax with executable code chunks:

```typst
= My Analysis

```{r setup}
#| eval: true
#| echo: true
library(tidyverse)
data <- iris
```

```{r summary}
#| eval: true
#| output: true
summary(data$Sepal.Length)
```
```

### Chunk Options

Common options (all languages):
- `eval: true/false` — Execute the code
- `echo: true/false` — Show the source code
- `output: true/false` — Show the result
- `cache: true/false` — Use caching (default: true)
- `label: <id>` — For cross-references
- `caption: "..."` — Figure/table caption
- `depends: [files...]` — External file dependencies

**Example with dependencies:**
```typst
```{r load-data}
#| eval: true
#| depends: data/raw.csv, scripts/utils.R
source("scripts/utils.R")
data <- read_csv("data/raw.csv") %>% clean_data()
```
```

If `data/raw.csv` or `scripts/utils.R` changes, the chunk is automatically re-executed.

### Rich Output with `typst()`

Knot includes an R package with the `typst()` generic function for rich output:

**DataFrames as Typst tables:**
```typst
```{r}
#| eval: true
#| echo: true
#| output: true
library(ggplot2)

# Create a summary table
summary_df <- data.frame(
  Metric = c("Mean", "Median", "SD"),
  Value = c(mean(iris$Sepal.Length),
            median(iris$Sepal.Length),
            sd(iris$Sepal.Length))
)

typst(summary_df)  # Renders as a Typst table
```
```

**ggplot2 plots as images:**
```typst
```{r}
#| eval: true
#| echo: true
#| output: true
gg <- ggplot(iris, aes(x = Sepal.Length, y = Sepal.Width, color = Species)) +
  geom_point(size = 3) +
  theme_minimal() +
  labs(title = "Iris Dataset")

# Configure dimensions and format
typst(gg, width = 8, height = 5, dpi = 300, format = "svg")
```
```

**Combined output (DataFrame + Plot):**
```typst
```{r}
#| eval: true
#| output: true
# Both will appear in the document
typst(summary_df)
typst(gg)
```
```

The `typst()` function:
- **Explicit and predictable** (no magic behavior)
- **Cacheable** (plots participate in cache system)
- **Configurable** (dimensions, resolution, format)
- **Consistent** (same pattern for DataFrames and plots)

## Architecture

Knot is built as a Rust workspace with clear separation:

```
knot/
├── crates/
│   ├── knot-core/        # Library (parsing, execution, caching)
│   └── knot-cli/         # CLI wrapper
├── knot-r-package/       # R package (typst() function)
├── knot-typst-package/   # Typst package for rendering
├── templates/            # Default document templates
└── examples/             # Example documents
```

### Intelligent Caching

Knot uses SHA256 chained hashing for cache invalidation:

```
Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + hash(dependencies))
```

**What's cached:**
- Text output from R
- DataFrames (as CSV files)
- Plots (SVG, PNG, PDF files)
- Combined outputs (DataFrame + Plot)

**Benefits:**
- Automatic cascade invalidation when code changes
- Implicit dependencies captured via chaining
- Explicit file dependencies via `depends` option
- Plot dimensions affect cache (change size → new plot generated)
- 100x-1000x speedup on cache hits

**Cache management:**
```bash
knot clean           # Clear all cache
knot clean --keep-metadata  # Keep metadata for inspection
```

## Roadmap

- [x] **Phase 1:** R code execution with persistent process
- [x] **Phase 2:** R package (`knot.r.package`) with `typst()` for DataFrames and plots
- [x] **Phase 3:** SHA256 chained caching with dependency tracking
- [x] **Phase 4:** Graphics support (ggplot2 with explicit control)
- [ ] **Phase 5:** Typst package publication (@preview/knot)
- [ ] **Phase 6:** Inline R expressions (`#r[expr]`), watch mode, global config
- [ ] **Phase 7:** LilyPond support for music notation
- [ ] **Phase 8:** Comprehensive tests and stabilization
- [ ] **Phase 9:** Community building and v1.0 release

**Current focus:** Phase 5-6 (Typst package + inline expressions)

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
