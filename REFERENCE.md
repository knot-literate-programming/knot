# Knot Reference Documentation

**Version**: 0.1.0 (pre-release)
**Last Updated**: 2026-02-01

This document describes the current state of the Knot project: architecture, implemented features, and design principles.

---

## Overview

**Knot** is a literate programming tool that executes code chunks (primarily R) embedded in Typst documents and generates formatted output.

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

Knot supports two usage modes:

1. **Simple Documents**: Standalone `.knot` files compiled directly
   - No configuration required
   - Defaults for all options
   - Cache stored in `.knot_cache/` relative to document
   - Ideal for quick analyses, single-file reports

2. **Structured Projects**: Multi-document projects with `knot.toml`
   - Project root detection (searches parent directories, like Cargo/Git)
   - Centralized configuration and defaults
   - Shared cache across all project documents
   - Organized structure for complex reports, books, courses

**Example Project Structure**:
```
my-analysis/
├── knot.toml              # Project config
├── main.knot              # Main document
├── chapters/
│   ├── intro.knot
│   └── results.knot
├── data/
│   └── dataset.csv
└── .knot_cache/           # Shared cache
```

**Philosophy**: Start simple, grow into structure. Knot doesn't force project setup until you need it.

---

## Architecture

### Crate Structure

```
knot/
├── crates/
│   ├── knot-core/        # Parser, compiler, executors, cache
│   ├── knot-cli/         # CLI interface (compile, init, clean)
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
| `executors/` | Language executors (R implemented, trait for others) |
| `backend/` | Format execution results as Typst code |
| `graphics.rs` | Graphics options resolution (defaults, config, chunk) |
| `config.rs` | Project configuration (knot.toml) |

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

**Example**:
````markdown
```{r my-plot}
#| caption: "Distribution of values"
#| fig-width: 8
#| fig-height: 6
hist(rnorm(1000))
```
````

### Inline Expressions

**Syntax**: `` `{language options} code` ``

**Supported Options**:
- `eval: bool` - Execute code (default: true)
- `echo: bool` - Show inline code (default: false)
- `digits: int` - Numeric precision for formatting

**Examples**:
- `` `{r} mean(x)` `` - Execute and insert result
- `` `{r digits=3} pi` `` - Format with 3 decimals
- `` `{r eval=false} dangerous()` `` - Skip execution
- `` `{r echo=true} sqrt(2)` `` - Show code alongside result

---

## Cache System

### Design Principles

1. **Content-addressed storage**: Results stored with SHA256 hash of:
   - Code content
   - Options (serialized JSON)
   - Previous chunk hash (sequential chaining)
   - Dependencies hash (file mtime + size)

2. **Sequential invalidation**: Changing chunk N invalidates all chunks N+1, N+2, ...

3. **Dependency tracking**: Chunks with `#| depends: file.csv` are invalidated when file changes

### Storage Structure

```
.knot_cache/
├── metadata.json           # Index of cached chunks/inline
├── chunk_0_stdout.txt      # Chunk output files
├── chunk_0_plot.svg
└── inline_abc123.txt       # Inline result files
```

### Cache Operations

```rust
// Compute hash (includes code, options, previous_hash, deps_hash)
let hash = cache.get_chunk_hash(code, &options, previous_hash, &deps_hash);

// Check cache
if cache.has_cached_result(&hash) {
    return cache.get_cached_result(&hash)?;
}

// Execute and save
let result = executor.execute(code)?;
cache.save_result(index, name, hash, &result, dependencies)?;
```

---

## Executors

### R Executor

**Implementation**: `crates/knot-core/src/executors/r/`

**Features**:
- Embedded R session (via `extendr` bindings)
- Sources `knot-r-package` for serialization helpers
- Parses stdout for special markers:
  - `__KNOT_SERIALIZED_CSV__` → DataFrame as CSV
  - `__KNOT_PLOT__` → Plot file path
  - Text → Plain text output

**Execution Flow**:
```rust
pub enum ExecutionResult {
    Text(String),
    DataFrame(String),     // CSV content
    Plot(String),          // File path
}

impl RExecutor {
    pub fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        // 1. Execute R code
        // 2. Capture stdout
        // 3. Parse markers
        // 4. Return structured result
    }
}
```

### Future Executors

The `LanguageExecutor` trait allows adding Python, Julia, etc.:

```rust
pub trait LanguageExecutor {
    fn execute(&mut self, code: &str) -> Result<ExecutionResult>;
    fn execute_inline(&mut self, code: &str) -> Result<String>;
}
```

---

## Backend (Typst Output)

### Chunk Formatting

**Implementation**: `crates/knot-core/src/backend/typst.rs`

Converts `ExecutionResult` to Typst code:

| Result Type | Typst Output |
|-------------|--------------|
| Text | `#code-chunk(code: "...", output: "...", ...)` |
| DataFrame | `#code-chunk(code: "...", tables: (#table(...)), ...)` |
| Plot | `#code-chunk(code: "...", images: ("path",), ...)` |

**Logic**:
- `echo=false` → omit `code:` parameter
- `output=false` → omit `output:` parameter
- `caption` + `name` → wrap in `#figure(caption: "...", <label>)[...]`

### Inline Formatting

Currently returns raw text result. Future: apply `digits` formatting.

---

## Configuration

### Project vs. Simple Mode

**Simple Mode** (no `knot.toml`):
- Compile single documents directly: `knot compile doc.knot`
- All defaults used
- Cache in `.knot_cache/` relative to document

**Project Mode** (with `knot.toml`):
- Centralized configuration for all project documents
- Automatic project root detection (searches parent directories)
- Shared cache across documents
- Custom defaults for graphics, R packages, etc.

### knot.toml Structure

```toml
[package]
name = "my-project"
version = "0.1.0"
authors = ["Name <email>"]

[helpers]
# Optional: Override vendored helpers with custom paths
# By default, knot uses lib/knot.R and lib/knot.typ
typst = "custom/path/to/knot.typ"  # Override Typst helper
r = "custom/path/to/knot.R"        # Override R helper

[graphics]
default_width = 7.0    # Default plot width (inches)
default_height = 5.0   # Default plot height (inches)
default_dpi = 300      # Default plot resolution
default_format = "svg" # Default plot format (svg|png)
```

### Helper Package Vendoring

**Philosophy**: Knot vendors (copies) helper packages into each project for reproducibility.

When you run `knot init my-project --project`, Knot creates:
```
my-project/
├── knot.toml
├── main.knot
└── lib/
    ├── knot.typ    # Vendored Typst helper
    └── knot.R      # Vendored R helper
```

**Why vendoring?**
1. **Guaranteed compatibility**: Helpers always match your knot CLI version
2. **Reproducibility**: Project is self-contained, works years later
3. **No external dependencies**: No need to install packages from CRAN/Typst Universe
4. **Offline support**: Compile without internet connection
5. **No version conflicts**: Each project has its own helpers

**Override if needed**: Use `[helpers]` section in `knot.toml` to point to:
- Global installation (reduce duplication for development)
- Custom modified helpers (advanced use cases)
- Shared helpers across multiple projects

**Default behavior**: If `[helpers]` is not specified, knot uses `lib/knot.typ` and `lib/knot.R`.

### Project Root Detection

The CLI searches parent directories for `knot.toml` (similar to Cargo/Git):

```bash
my-course/
├── knot.toml          # Project root
└── chapters/
    └── chapter1.knot

$ cd my-course/chapters/
$ knot compile chapter1.knot
# ✓ Finds knot.toml in ../
# ✓ Uses project configuration
# ✓ Cache shared at ../knot_cache/
```

This allows organizing complex projects with multiple documents while maintaining centralized settings.

### Option Resolution Priority

For any configurable option (e.g., graphics settings), the priority is:

1. **Chunk options** (`#| fig-width: 10`) — highest priority
2. **Config file** (`[graphics]` section in `knot.toml`)
3. **Hardcoded defaults** (`GraphicsDefaults::default()`) — lowest priority

This ensures flexibility: chunk-level control overrides project defaults, which override system defaults.

---

## LSP Server

**Implementation**: `crates/knot-lsp/`

**Features**:
- ✅ Diagnostics (parsing errors, execution errors)
- ✅ Hover (chunk info, inline expression info)
- ✅ Code lenses ("Execute chunk", "Clear cache")
- ✅ Completion (chunk options, inline options)
- ⚠️ Workspace symbol navigation (partial)

**Protocol**: Language Server Protocol via `tower-lsp`

---

## Testing

### Test Coverage

**Total Tests**: 89 passing (as of 2026-02-01)

| Module | Tests | Coverage |
|--------|-------|----------|
| `parser` | 13 | Chunks, inline, options parsing |
| `cache` | 3 | Hashing, chaining, dependencies |
| `compiler/chunk_processor` | 12 | Execution, caching, hashing |
| `compiler/inline_processor` | 12 | Options, eval, caching |
| `executors/r/output_parser` | 15 | CSV/plot markers, parsing |
| `backend` | 11 | Typst formatting |
| `graphics` | 4 | Option resolution |
| `config` | 11 | Config parsing |
| LSP handlers | 8 | Diagnostics, hover, completion |

### Test Philosophy

- **Unit tests**: Pure logic (parsing, hashing, formatting)
- **Integration tests**: Require R installation (marked with `#[ignore]`)
- **Examples**: Real-world `.knot` files in `examples/`

### Running Tests

```bash
# All tests (skip R integration tests)
cargo test

# Include R integration tests (requires R + knot package)
cargo test -- --include-ignored

# Specific module
cargo test --package knot-core --lib parser
```

---

## CLI Commands

### Basic Usage

```bash
# Compile a document
knot compile document.knot

# Initialize a simple document
knot init article.knot

# Initialize a structured project
knot init my-course --project

# Clear cache
knot clean
```

### Compile Options

```bash
# Force recompilation (ignore cache)
knot compile document.knot --no-cache

# Specify output file
knot compile document.knot --output custom-name.typ

# Verbose logging
RUST_LOG=debug knot compile document.knot
```

### Project Initialization

**Simple Document** (`knot init article.knot`):
- Creates standalone `.knot` file from template
- No project structure
- Quick start for single-file reports

**Structured Project** (`knot init my-project --project`):
- Creates project directory with `knot.toml`
- **Vendors helper packages** for reproducibility
- Sets up recommended structure:
  ```
  my-project/
  ├── knot.toml         # Project configuration
  ├── main.knot         # Main document
  ├── lib/              # Vendored helpers (auto-copied)
  │   ├── knot.typ      # Typst helper functions
  │   └── knot.R        # R helper functions
  ├── chapters/         # Additional documents (optional)
  ├── data/             # Data files (optional)
  └── .gitignore        # Recommended Git ignore patterns
  ```
- Configures defaults and package metadata

**What gets vendored**:
- `lib/knot.typ`: Typst helper (matching CLI version)
- `lib/knot.R`: R helper (matching CLI version)

This ensures your project will compile identically even if you upgrade knot CLI later.

**Philosophy**: Use simple mode for one-off analyses. Upgrade to project mode when you need:
- Multiple related documents
- Custom default settings
- Shared cache across files
- Version control and collaboration

---

## Development Status

### Implemented ✅

- [x] Parse `.knot` files (chunks + inline expressions)
- [x] Execute R code with embedded session
- [x] Cache system with SHA256 hashing
- [x] Sequential cache invalidation
- [x] Dependency tracking for cache
- [x] Chunk options (eval, echo, output, cache, depends, caption, label)
- [x] Inline options (eval, echo, digits)
- [x] Graphics options (parsing and resolution)
- [x] DataFrame → CSV → Typst table
- [x] Plot generation (via R package)
- [x] Typst backend formatting
- [x] CLI (compile, init, clean)
- [x] LSP server (diagnostics, hover, completion, code lens)
- [x] Comprehensive test suite (89 tests)

### Partially Implemented 🚧

- [ ] Graphics generation (options parsed, generation not wired up)
- [ ] Inline `digits` formatting (option parsed, not applied)
- [ ] Document-level config YAML frontmatter

### Not Yet Implemented ❌

- [ ] Python/Julia executors
- [ ] Watch mode (auto-recompile on file changes)
- [ ] Incremental compilation
- [ ] Advanced LSP features (rename, find references)

---

## Design Principles

### 1. Explicit over Implicit

Options have clear defaults. No "magic" behavior.

### 2. Content-Addressable Cache

Results are cached by content hash, not file location. Moving chunks doesn't break cache.

### 3. Sequential Execution

Chunks execute in document order. Later chunks see variables from earlier chunks.

### 4. Fail Fast

Parsing/execution errors stop compilation immediately with clear error messages.

### 5. Typst-First

Output is optimized for Typst. No attempt to support multiple backends.

---

## File Formats

### .knot → .typ Pipeline

1. **Parse** `.knot` → `Document` (chunks + inline expressions)
2. **Execute** each chunk → `ExecutionResult`
3. **Format** results → Typst code snippets
4. **Assemble** snippets → `.typ` output
5. **Copy** plots/CSVs to `_knot_files/`

### Generated Files

| File | Purpose |
|------|---------|
| `output.typ` | Typst source with embedded results |
| `_knot_files/` | Plots, CSVs referenced by `output.typ` |
| `.knot_cache/` | Cached execution results |

---

## Common Patterns

### Adding a New Chunk Option

1. Add field to `ChunkOptions` struct (parser.rs)
2. Add parsing logic in `parse_chunk_options()` (parser.rs)
3. Update `ChunkOptions` serialization (for cache hashing)
4. Use option in `chunk_processor.rs` or `backend.rs`
5. Add tests

### Adding a New Language Executor

1. Implement `LanguageExecutor` trait
2. Add executor to `ExecutorManager` in compiler
3. Update `process_chunk()` to handle new language
4. Add tests

### Debugging Cache Issues

1. Check hash computation: `cache.get_chunk_hash()`
2. Inspect `.knot_cache/metadata.json`
3. Use `knot clean` to clear cache
4. Run with logging: `RUST_LOG=debug knot compile`

---

## Known Limitations

1. **R-only**: Python/Julia executors not yet implemented
2. **No streaming**: Large outputs are buffered in memory
3. **Single-threaded**: Chunks execute sequentially (by design)
4. **No sandboxing**: Executed code has full system access
5. **Typst-only**: No Markdown/LaTeX/HTML output

---

## Future Directions

(See `ROADMAP.md` for detailed plans)

- **Phase 5**: Python executor
- **Phase 6**: Watch mode and incremental compilation
- **Phase 7**: Advanced LSP features
- **v1.0**: Stable API, comprehensive documentation

### Robustness: Side Channel Execution

Currently, Knot parses standard output (stdout) for magic strings like `__KNOT_PLOT__`. This is fragile as user code might accidentally print these strings.

**Proposed Improvement:** Use a "Side Channel" for metadata.
1. Knot creates a temporary JSON file path and passes it to R via env var `KNOT_META_FILE`.
2. The `typst()` R function writes structured metadata (plots, tables) to this JSON file instead of stdout.
3. User output (stdout) remains purely for logs/text.
4. Knot reads the JSON file after execution to retrieve artifacts.

---

## Resources

- **AI Context**: See `AI_CONTEXT.md` for development conventions
- **Workflow**: See `WARP.md` for Git workflow and branch strategy
- **Examples**: See `examples/` for sample `.knot` files
- **Old Reference**: See `REFERENCE_old.md` for historical design notes

---

**Document Status**: Active (reflects current implementation)
**Obsolete Reference**: `REFERENCE_old.md` (to be removed in v1.0)
