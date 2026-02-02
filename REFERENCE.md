# Knot Reference Documentation

**Version**: 0.1.0 (pre-release)
**Last Updated**: 2026-02-02

This document describes the current state of the Knot project: architecture, implemented features, and design principles.

---

## Overview

**Knot** is a reproducible document engineering tool. It executes code chunks (primarily R) embedded in Typst documents and generates formatted output. It is designed for serious reporting, thesis writing, and technical documentation where reproducibility is paramount.

### Core Concept

```knot
= My Analysis

```{r data-load}
#| eval: true
#| echo: false
data <- read.csv("data.csv")
summary(data)
```

The mean is `{r} mean(data$x)`.
```

Knot parses `.knot` files, executes code chunks, caches results, and generates `.typ` output with embedded tables, plots, and inline values.

### Project Philosophy

Knot strictly separates "Single File" usage from "Project" usage.

1. **Simple Documents**: Standalone `.knot` files compiled directly.
   - Command: `knot compile doc.knot`
   - No `knot.toml` needed (optional).
   - Generates a hidden `.doc.typ` file and runs Typst.
   - Ideal for quick analyses.

2. **Structured Projects**: Multi-document projects defined in `knot.toml`.
   - Command: `knot build`
   - **Isolation**: Each included `.knot` file is compiled independently. No shared variables between files.
   - **Composition**: The `main.knot` serves as the entry point and layout skeleton.
   - **Injection**: Knot automatically injects the compiled files into the main document.

**Example Project Structure**:
```
my-thesis/
├── knot.toml              # Project definition
├── main.knot              # Main layout (title, template, placeholder)
├── includes/
│   ├── 01-intro.knot      # Independent component
│   └── 02-results.knot    # Independent component
├── data/
│   └── dataset.csv
└── lib/                   # Helpers
```

**Philosophy**: **Strict Reproducibility**. No "Play" button. Linear execution. No shared state between chapters unless explicitly sourced from a common script.

---

## Architecture

### Crate Structure

```
knot/
├── crates/
│   ├── knot-core/        # Parser, compiler, executors, cache
│   ├── knot-cli/         # CLI interface (build, compile, init, watch)
│   └── knot-lsp/         # LSP server (diagnostics, completion)
├── knot-r-package/       # R helper package for serialization
├── knot-typst-package/   # Typst package for rendering
└── templates/            # Default templates
```

### knot-core Modules

| Module | Purpose |
|--------|---------|
| `parser.rs` | Parse `.knot` files into chunks and inline expressions |
| `compiler/` | Compile chunks/inline → Typst output |
| `cache/` | SHA256-based content cache with sequential invalidation |
| `executors/` | Language executors (R implemented, side-channel communication) |
| `backend/` | Format execution results as Typst code |
| `graphics.rs` | Graphics options resolution (defaults, config, chunk) |
| `config.rs` | Project configuration (knot.toml) |

---

## Configuration (knot.toml)

The `knot.toml` file is the heart of a project.

```toml
[package]
name = "my-thesis"
version = "0.1.0"
authors = ["Nicolas <me@example.com>"]

[document]
main = "main.knot"
includes = [
    "chapters/01-intro.knot",
    "chapters/02-methods.knot",
    "chapters/03-results.knot"
]

[helpers]
typst = "lib/knot.typ"
r = "lib/knot.R"

[defaults]
fig-width = 6.0
fig-height = 4.0
cache = true
```

### Multi-File Build Logic

When running `knot build`:
1. Knot reads `includes` list from `knot.toml`.
2. Validates that all included files are within the project root (security).
3. Each included file is compiled **independently** to a hidden `.typ` file (e.g., `chapters/.01-intro.typ`).
4. `main.knot` is compiled to `.main.typ`.
5. Knot looks for the **mandatory** placeholder `/* KNOT-INJECT-CHAPTERS */` in `.main.typ` and replaces it with Typst `#include` directives pointing to the compiled files.
6. Finally, `typst compile` is run on `.main.typ` to generate the PDF.

**Important**:
- A `.knot` file should **never** include another `.knot` file directly. Use `knot.toml` to structure your document.
- The `/* KNOT-INJECT-CHAPTERS */` placeholder is **mandatory** in `main.knot` when `includes` are present in `knot.toml`.

---

## Syntax

### Code Chunks

````markdown
```{language name}
#| option: value
#| option: value
code here
```
````

**Supported Options**:
- `eval: bool` - Execute chunk (default: true)
- `echo: bool` - Show source code (default: true)
- `output: bool` - Show execution output (default: true)
- `cache: bool` - Cache results (default: true)
- `depends: path, path` - File dependencies for cache invalidation
- `caption: "text"` - Figure caption (wraps chunk in `#figure`)
- `label: <name>` - Typst label for references
- `fig-width: float` - Plot width in inches
- `fig-height: float` - Plot height in inches
- `dpi: int` - Plot resolution
- `fig-format: "svg"|"png"` - Plot output format
- `fig-alt: "text"` - Alt text for accessibility

### Inline Expressions

**Syntax**: `` `{language options} code` ``

**Examples**:
- `` `{r} mean(x)` `` - Execute and insert result
- `` `{r digits=3} pi` `` - Format with 3 decimals

---

## Cache System

### Design Principles

1. **Content-addressed storage**: Results stored with SHA256 hash.
2. **Sequential invalidation**: Changing chunk N invalidates all subsequent chunks in that file.
3. **Dependency tracking**: Chunks with `#| depends: file.csv` are invalidated when file changes.

### Storage Structure

```
.knot_cache/
├── metadata.json           # Index of cached chunks/inline
├── chunk_0_stdout.txt      # Chunk output files
├── chunk_0_plot.svg
└── inline_abc123.txt       # Inline result files
```

---

## Executors

### R Executor

**Implementation**: `crates/knot-core/src/executors/r/`

**Robustness**: Uses a **Side-Channel** approach.
1. Knot creates a temporary JSON file path.
2. Passes it to R via `KNOT_METADATA_FILE` environment variable.
3. R writes metadata (paths to plots, tables) to this JSON file.
4. Rust reads the JSON to reconstruct the execution result.

This avoids fragile stdout parsing and ensures robust communication.

---

## CLI Commands

### `knot init <name> [--project]`
Initializes a new project.
- Creates `knot.toml`, `main.knot`.
- Vendors `lib/knot.typ` and `lib/knot.R` (ensuring reproducibility).

### `knot compile <file>`
Compiles a single `.knot` file to a hidden `.typ` file. Does not generate PDF by default (unless you run `typst compile` afterwards). Useful for debugging a specific chapter.

### `knot build`
Builds the entire project defined in `knot.toml`.
1. Compiles all chapters.
2. Compiles main file.
3. Injects includes.
4. Generates PDF via Typst.

### `knot watch`
Watches for changes.
- Currently launches `typst watch` on the generated files.
- **Note**: Automatic re-execution of R code on change is partially implemented (requires manual trigger for now).

### `knot clean`
Clears the `.knot_cache` directory.

---

## Development Status

### Implemented ✅

- [x] Parse `.knot` files (chunks + inline expressions)
- [x] Execute R code with embedded session
- [x] Cache system with SHA256 hashing & sequential invalidation
- [x] Side-Channel communication for robust R output
- [x] **Multi-file project support (knot.toml)**
- [x] Chunk options (eval, echo, output, cache, depends, caption, label)
- [x] Graphics options (parsing and resolution)
- [x] DataFrame → CSV → Typst table
- [x] Plot generation (via R package)
- [x] Typst backend formatting
- [x] CLI (build, compile, init, clean)
- [x] LSP server (diagnostics, hover, completion)

### Partially Implemented 🚧

- [ ] **Watch Mode**: Currently delegates to `typst watch`. Needs to watch `.knot` files and trigger `knot build` automatically.

### Not Yet Implemented ❌

- [ ] Python/Julia executors
- [ ] Incremental compilation (parallel builds of chapters)

---

## Known Limitations

1. **R-only**: Python/Julia executors not yet implemented.
2. **Single-threaded**: Chunks execute sequentially (by design). Chapters *could* be compiled in parallel (future optimization).
3. **No shared state between chapters**: You must `source("setup.R")` in each chapter if you need common data. This is a feature, not a bug, for reproducibility.

---

**Document Status**: Active (reflects current implementation)