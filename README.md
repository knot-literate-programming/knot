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

**v0.1.0-alpha** — Phases 1 and 3 complete

✅ **Completed:**
- R code execution with persistent R process
- Chunk parsing with `#|` options (Quarto-style)
- SHA256 chained caching system with automatic invalidation
- File dependency tracking (`#| depends:`)
- Cross-referencing chunks with `@chunk-name`
- LSP-ready parser with position tracking

🚧 **In Progress:**
- Phase 2: R package for rich output (data frames, plots)
- Phase 4: Graphics support (ggplot2, base R plots)

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/knot.git
cd knot

# Build the project
cargo build --release

# Install (optional)
cargo install --path crates/knot-cli
```

### Prerequisites

- **Rust** 1.70+ ([rustup](https://rustup.rs/))
- **Typst** ([installation](https://github.com/typst/typst))
- **R** (for R code execution)
- **typstyle** (for formatting): `cargo install typstyle`

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

## Architecture

Knot is built as a Rust workspace with clear separation:

```
knot/
├── crates/
│   ├── knot-core/     # Library (parsing, execution, caching)
│   └── knot-cli/      # CLI wrapper
├── templates/         # Default document templates
├── examples/          # Example documents
└── knot-typst-package/  # Typst package for rendering
```

### Intelligent Caching

Knot uses SHA256 chained hashing for cache invalidation:

```
Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + hash(dependencies))
```

**Benefits:**
- Automatic cascade invalidation when code changes
- Implicit dependencies captured via chaining
- Explicit file dependencies via `depends` option
- 100x-1000x speedup on cache hits

## Roadmap

- [x] **Phase 1:** R code execution (subprocess)
- [x] **Phase 3:** Chained caching system
- [ ] **Phase 2:** R package for rich outputs
- [ ] **Phase 4:** Graphics support (ggplot2, plots)
- [ ] **Phase 5:** Typst package (@preview/knot)
- [ ] **Phase 6:** Inline expressions, watch mode
- [ ] **Phase 7:** LilyPond support
- [ ] **Phase 8:** Tests and stabilization
- [ ] **Phase 9:** Community and v1.0 release

See [knot-project-reference.txt](knot-project-reference.txt) for the complete roadmap.

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
- Recursive naming inspired by GNU ("GNU's Not Unix")

---

**"knot is not knitr"** — Literate programming for the Typst era
