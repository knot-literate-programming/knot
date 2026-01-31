# AI Context & Guidelines

This file contains the conventions, workflow, and quick pointers for AI assistants working on the Knot project.

## 🔴 Read This First

**IMPORTANT**: Before starting to code, read in this order:
1. **`AI_CONTEXT.md`** (this file) - conventions and workflow
2. **`REFERENCE.md`** - comprehensive project reference, architecture, roadmap, and development log.

These two files provide the complete context to resume development.

## Development Workflow

### Tests and Validation
- **Always** create examples in `examples/` rather than isolated tests
- Run `cargo test` before each commit
- Tests must cover parsing AND option resolution

### Commits
- Format commits with co-author:
  ```
  git commit -m "$(cat <<'EOF'
  type(scope): description

  Co-Authored-By: AI Assistant <noreply@example.com>
  EOF
  )"
  ```
- Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

### Creating Files
- **DO NOT** create markdown or documentation files without explicit request
- **ALWAYS** prefer editing an existing file rather than creating a new one
- New modules must be exported in `lib.rs`

## Advice for Major Refactorings

**IMPORTANT**: When performing a major refactoring or architectural changes, it is **MANDATORY** to create a dedicated Git branch. This isolates the work, allows for easy rollback (without impacting `master`), and avoids complex history management issues.

**Recommended Workflow:**
1.  `git checkout -b feature/my-refactoring`
2.  Make changes
3.  Commit on this branch
4.  Once refactoring is complete and tested, merge into `master`.

## Key Project Structure

### Core (`crates/knot-core/src/`)
- **`parser.rs`** : Parsing chunks and options (`ChunkOptions`, `Document::parse()`)
- **`executors/mod.rs`** : Enum `ExecutionResult` and trait `Executor`
- **`executors/r.rs`** : R executor with cache management and knot package
- **`compiler.rs`** : Generation of final `.typ` from `.knot`
- **`cache.rs`** : SHA256-based cache system with chaining
- **`graphics.rs`** : Graphics options (defaults, config, resolution)

### CLI (`crates/knot-cli/src/`)
- **`main.rs`** : Entry point, commands `compile`, `init`, `clean`
- Function `fix_paths_in_typst()` : Copies CSVs to `_knot_files/`

### R Package (`knot-r-package/`)
- **`R/typst.R`** : S3 methods for conversion (DataFrames → CSV, Plots → SVG/PNG)
- **`NAMESPACE`** : Required S3method exports

### Typst Package (`knot-typst-package/`)
- **`lib.typ`** : Function `#code-chunk()` for display
- Requires `#show: codly-init` in documents

## Code Conventions

### Chunk Options
- Stored in `ChunkOptions` (parser.rs)
- New options: add to struct + parser + tests
- Naming: kebab-case in `.knot` → snake_case in Rust (`fig-width` → `fig_width`)

### Option Hierarchy
Priority (highest first):
1. Chunk level options (`#| fig-width: 10`)
2. Document config YAML frontmatter (future)
3. Hardcoded defaults (`GraphicsDefaults::default()`)

### Generated File Management
- Cache: `.knot_cache/` with SHA256-based names
- Output files: `_knot_files/` (copy from cache)
- Knitr-style pattern for compatibility

## Resolution Patterns

### Graphics options
```rust
let defaults = GraphicsDefaults::default();
let doc_graphics = None; // or Some(GraphicsConfig { ... })
let resolved = resolve_graphics_options(&chunk.options, &doc_graphics, &defaults);
```

### Execution and cache
```rust
// Check cache first
if let Some(cached) = cache_manager.get_cached_result(&chunk) {
    return Ok(cached);
}

// Else execute and cache
let result = executor.execute_chunk(&chunk)?;
cache_manager.cache_result(&chunk, &result)?;
```

## Project Phases

See `REFERENCE.md` for full details.

- **Phase 1** : ✅ Basic R execution
- **Phase 2** : ✅ R Package and DataFrames → Typst tables
- **Phase 3** : ✅ Cache system
- **Phase 4** : 🚧 Graphics (4A: bitmap/vector, 4B: native Typst)
  - Infrastructure options: ✅ Parsing and resolution
  - Generation: ❌ To implement

## Important Reminders

- The chunk parsing regex is in `crates/knot-core/src/lib.rs` (`CHUNK_REGEX`)
- CSV marker in R stdout: `__KNOT_SERIALIZED_CSV__`
- Typst table syntax: `#table(columns: data.first().len(), ..csv("path").flatten())`
- Default template: `templates/default.knot`
