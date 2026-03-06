# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project.
Detailed designs for specific features are located in their respective files
within this directory; completed plans are in `archives/`.

## Project Overview

- **`knot-core`**: The engine (parser, compiler, cache, language executors).
- **`knot-lsp`**: Language Server for IDE support (proxies Tinymist + Knot-specific features).
- **`knot-cli`**: Command-line interface (`knot build`, `watch`, `init`, `clean`, sync commands).
- **`editors/vscode/`**: VS Code extension (TypeScript).

---

## Current Status (March 2026)

**Maturity:** Feature-complete for v1.0 scope. Focus is now on documentation,
contributor experience, and polish before public launch.

### knot-core — Complete

- Parsing (winnow-based combinator parser for `.knot` format)
- Three-pass compilation pipeline: Plan → Execute → Assemble
- Parallel execution: R and Python run in separate OS threads simultaneously
- Progressive/streaming compilation: Phase 0 (instant cache-hit preview) +
  per-chunk `ProgressEvent` streaming for live preview
- Intelligent SHA-256 chained cache (editing chunk N invalidates N+1, N+2, …)
- Rich output: text, plots (SVG/PNG), DataFrames → Typst tables, mixed types
- Multi-language: R and Python in the same document
- Freeze feature: hash contracts on named objects across chunks
- Full chunk options system (YAML `#|` frontmatter, per-language defaults, global defaults)
- Sync mapping: bidirectional source ↔ PDF navigation via `#KNOT-SYNC` markers
- Rust best practices: workspace lints, `#[expect]`, public API docs, snapshot tests,
  clone elimination, no `unwrap`/`expect` in production paths, no tokio dependency

### knot-lsp — Near-complete

- Proxy to Tinymist with `.knot` ↔ virtual `.typ` coordinate translation
- Chunk-option completion and hover docs
- Hybrid formatting: Air (R) + Ruff (Python) + Tinymist (Typst)
- Diagnostics: parse errors, unknown options, runtime errors from cache
- Document symbols
- Streaming preview: Phase 0 (instant) + per-chunk updates via `TinymistOverlay`
- Forward sync: cursor in `.knot` → scroll PDF preview
- Backward sync: click in PDF → open `.knot` at matching line
- Generation guard (`compile_generation`) discards stale in-flight compiles
- `do_phase0_only` on `didChange` (amber borders for modified chunks)
- `do_compile` on `didSave` / Run button (orange borders, then real output)

  **Pending:**
  - Go to Definition (Typst symbols) — see `lsp-navigation.md`
  - Find References

### knot-cli — Complete

- `knot build`: one-shot compilation to PDF via `typst compile`
- `knot watch`: file-change watch + `typst watch` subprocess
- `knot watch --preview`: watch + Tinymist preview
- `knot init`: project scaffolding with `knot.toml`, `lib/`, `main.knot`
- `knot clean`: wipe cache
- `knot jump-to-source <typ_file> <line>`: `.typ` line → `.knot` file + line
- `knot jump-to-typ <typ_file> <knot_file> <line>`: `.knot` line → `.typ` line

---

## Current Priorities

### 1. Documentation and contributor onboarding — Complete

- [x] User guide (installation, first project, all chunk options, VS Code guide)
- [x] Developer guide (architecture deep-dive, how to add a language executor,
      how to add a chunk option)
- [x] Contributor install script (check prerequisites: R, Python, Typst, Tinymist, Rust)
- [x] Update README with demo GIF/screenshot, clearer quick-start
- [x] `mdbook` setup for hosted documentation (GitHub Pages)

### 2. LSP navigation (Go to Definition / References)

See `lsp-navigation.md`. Intercept `textDocument/definition` and
`textDocument/references`, map coordinates through `PositionMapper`, forward
to Tinymist, and map the response back. See Issue #5.

### 3. Pre-launch checklist — Complete

See `pre-launch-checklist.md`: CI (already in place), binary distribution
via `cargo-dist` (configured for macOS, Linux, Windows), review for sensitive information before public push.

### 4. Future / post-v1.0

- Julia language executor (Issue #3)
- Variable Explorer (live introspection of R/Python sessions in the editor)
- Content generators (Mermaid diagrams, LilyPond, …)
- Improved error messages for CI/CD environments
- Replace anyhow with thiserror in `knot-core` (Issue #1)

---

## Technical Debt

These are known weaknesses to address after v1.0, in priority order.

### A. Integration test coverage (high priority)

All R/Python integration tests are `#[ignore]` — the most critical execution
logic is not verified in CI. The fix requires mock executors or a CI environment
with R and Python installed.

### B. Typed errors in the public API (medium priority)

`knot-core`'s public API returns `anyhow::Result` everywhere. For a published
library this is suboptimal: callers cannot pattern-match on specific error types.

**Step 1** (targeted): Define a `ProjectError` enum in `project.rs` using
`thiserror` for the variants callers might want to distinguish
(`ConfigNotFound`, `MainFileNotFound`, `IncludeOutsideRoot`). Internal
`anyhow` errors wrapped via `#[error(transparent)]`.

**Step 2** (full): Propagate typed errors through `compiler/`, `cache/`,
`parser/`, `executors/`, replacing `anyhow` throughout `knot-core`.

Deferred until callers (LSP, CLI) actually need to branch on error variants.
See the TODO comment in `crates/knot-core/src/project.rs`.

### C. compile_project_full complexity (low priority)

The streaming branch of `compile_project_full` in `project.rs` has high
cyclomatic complexity. Could be split into named helpers without changing
the external API.

### D. Box<dyn Fn> callback (low priority)

The `on_progress` parameter of `compile_project_full` is
`Option<Box<dyn Fn(String) + Send>>`. An `impl Fn(String) + Send` generic
parameter would avoid the heap allocation and be more idiomatic, but requires
a type parameter on the function which may complicate call sites.

---

## Design Documents

### Active

- `lsp-navigation.md`: Roadmap for Go to Definition and References.
- `pre-launch-checklist.md`: Steps before the public GitHub launch.

### Archived (completed)

- `archives/async-parallel-pipeline.md`: Parallel R/Python execution design.
  Implemented in `refactor/async-prereqs` and `feat/progressive-compilation`.
- `archives/progressive-compilation.md`: Initial progressive compilation vision.
- `archives/knot-preview-plan.md`: LSP streaming preview (Phase 0 + per-chunk
  updates, `TinymistOverlay`, `compile_generation`). Fully implemented.
- `archives/sync-mapping.md`: PDF ↔ source sync design (initial draft used
  `#BEGIN-CHUNK` markers; final implementation uses `#KNOT-SYNC` markers and
  `knot jump-to-source` / `knot jump-to-typ` CLI commands).
- `archives/codebase-hardening-2026-02.md`: C1–C3, D1–D3, De1–De5, Q1–Q9 fixes.
- `archives/lsp-diagnostics.md`: Runtime diagnostics in the editor.
- `archives/r-error-handling.md`, `archives/python-error-handling.md`: Unified error model.
- `archives/formatters.md`: Air + Ruff + Tinymist hybrid formatting.
- `archives/chunk-customization.md`: Flexible chunk presentation options.
- `archives/execution-output-refactor.md`: `ExecutionAttempt` / `ExecutionOutput` refactor.
