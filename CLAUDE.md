# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Knot

Knot is a literate programming system for [Typst](https://typst.app): it lets users embed executable R and Python code blocks directly in `.knot` documents, compiling them into `.typ` files for Typst to render as PDF. Think RMarkdown, but with Typst instead of LaTeX and a Rust engine.

The workspace has three Rust crates plus a VS Code extension:
- **`knot-core`** — parser, compiler, cache, and language executors
- **`knot-cli`** — `knot` command-line tool
- **`knot-lsp`** — Language Server (proxies Tinymist + adds Knot-specific features)
- **`editors/vscode/`** — VS Code extension (TypeScript)

## Commands

```bash
# Build
cargo build --release          # produces target/release/knot and knot-lsp

# Test
cargo test --workspace
cargo test -p knot-core        # single crate

# Lint / format
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# VS Code extension
cd editors/vscode && npm install && npm run compile
npm run package                # produces .vsix

# Integration testing
cargo run -- compile examples/consolidated/main.knot
cargo run -- build             # full project → .typ + PDF (via typst compile)
cargo run -- watch             # watch mode + typst watch for live PDF
cargo run -- watch --preview   # watch mode + tinymist preview
cargo run -- clean             # wipe cache
```

### knot build vs knot watch vs VS Code preview

| Mode | Trigger | PDF generation | Streaming |
|------|---------|----------------|-----------|
| `knot build` | manual | `typst compile` (one-shot) | no |
| `knot watch` | file change | `typst watch` subprocess | no |
| `knot watch --preview` | file change | `tinymist preview` subprocess | no |
| VS Code preview | `didSave` / Run button | LSP → our Tinymist subprocess | yes |

`knot build` and `knot watch` use `compile_project_full(path, None)` (non-streaming). VS Code preview uses `compile_project_full(path, Some(tx))` (streaming) so chunks appear as they complete.

## Before committing

Simulate the three CI jobs locally (matching `.github/workflows/ci.yml`) in this order:

```bash
# 1. Check & Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# 2. Tests (knot-cli excluded: its integration tests require R/Python)
cargo test --workspace --exclude knot-cli

# 3. VS Code extension
cd editors/vscode && npm ci && npm run compile
```

## Three-Pass Compilation Pipeline

The core loop lives in `crates/knot-core/src/compiler/`:

**Pass 1 – Planning** (`pipeline.rs`): Parse the `.knot` document, resolve chunk options, compute SHA-256 hashes (chained sequentially so that editing chunk N invalidates N+1, N+2, …), and classify each node as `Skip`, `CacheHit`, or `MustExecute`.

**Pass 2 – Execution** (`execution.rs`): Chunks tagged `MustExecute` are grouped by language via `group_by_language()`, then R and Python chains run **in parallel** via `std::thread::scope`. Within each language chain execution is sequential (preserving interpreter state). Results are written to cache.

**Pass 3 – Assembly** (`mod.rs`): Node outputs are interleaved with the source text to produce a `.typ` file consumed by Typst.

Key types:
```
ExecutionNeed        ::= Skip | CacheHit(ExecutionAttempt) | MustExecute
ExecutionAttempt     ::= Success(ExecutionOutput) | RuntimeError(RuntimeError)
ChunkExecutionState  ::= Ready            -- cache hit or just executed
                       | Inert            -- suspended due to upstream error
                       | Pending          -- do_compile Phase 0: orange border
                       | Modified         -- do_phase0_only: first MustExecute in chain (amber strong)
                       | ModifiedCascade  -- do_phase0_only: subsequent MustExecute in chain (amber muted)
Phase0Mode           ::= Pending | Modified
```

**Cascade Inert**: when a chunk in language L errors, all subsequent L-chunks become `Inert` (state is uncertain).

### Progressive Compilation

The compiler supports a two-phase API for live preview:

**Phase 0** (`plan_and_partial` in `mod.rs`): runs Pass 1 only — no code executed. Cache hits render with real output; `MustExecute` nodes render as visual placeholders controlled by `Phase0Mode`:
- `Phase0Mode::Pending` — compilation is in progress (`do_compile`): all pending chunks show orange border.
- `Phase0Mode::Modified` — user is editing without compiling (`do_phase0_only`): first `MustExecute` per language chain shows amber (strong), subsequent ones amber (muted) to distinguish direct edits from hash-cascade invalidations.

**Phase 0 project-level** (`project.rs`):
- `compile_project_phase0(path, mode)` — reads from disk, instant.
- `compile_project_phase0_unsaved(path, unsaved_path, content, mode)` — uses in-memory buffer for one file (typing-time updates).

**Streaming execution** (`execute_and_assemble_streaming` + `ProgressEvent`): after Phase 0, Pass 2 runs in a separate thread and emits a `ProgressEvent { doc_idx, executed }` after each node completes. The caller replaces the matching entry in the partial buffer and re-assembles, pushing incremental `.typ` updates to Tinymist.

**Full project** (`compile_project_full(path, on_progress)`): assembles includes + main file; passes streaming sender if provided. Used by `knot watch` (no streaming) and the LSP (streaming).

## Cache

`crates/knot-core/src/cache/` — SHA-256 addressed, persisted as `.knot_cache/metadata.json`. The hash of chunk N includes the hash of chunk N-1, so any change cascades invalidations forward.

`ExecutorManager` (in `executors/manager.rs`) uses a take/put-back pattern so executors can be moved into threads safely.

## Language Executors

Both executors (`executors/python/`, `executors/r/`) follow the same pattern:

1. Spawn a persistent subprocess running the embedded helper scripts (`resources/python/`, `resources/r/`).
2. Before executing user code, set environment variables in the child process:
   - `KNOT_METADATA_FILE` — path to side-channel temp JSON (for graphics/DataFrame metadata)
   - `KNOT_CACHE_DIR`, `KNOT_FIG_WIDTH/HEIGHT/DPI/FORMAT`
3. Send code, read stdout/stderr, read side-channel JSON for rich output (plots, tables).
4. Return `ExecutionAttempt`.

The side-channel (`executors/side_channel.rs`) is a temporary JSON file that lets the language runtime pass structured metadata (figure paths, DataFrame HTML, etc.) back to Rust without shell-escaping issues.

## LSP Architecture

`knot-lsp` is a **proxy to Tinymist** (the official Typst LSP). It:

- Intercepts LSP requests from the editor, transforms `.knot` coordinates to virtual `.typ` coordinates via `position_mapper.rs`, forwards them to a Tinymist subprocess, and transforms responses back.
- Adds Knot-specific features directly: chunk-option completion (`handlers/completion.rs`), hover docs (`handlers/hover.rs`), document formatting (Air for R, Ruff for Python, Tinymist for Typst via `handlers/formatting.rs`), diagnostics merging (`diagnostics.rs`), and document symbols (`symbols.rs`).
- Manages preview and sync via `knot/startPreview` and `knot/syncForward` custom LSP methods (see `sync.rs`).

`ServerState` uses `Arc<RwLock<>>` throughout for concurrent access.

### LSP Preview Architecture

`knot/startPreview` starts a Tinymist preview task **in our own Tinymist subprocess** (not the VS Code extension's). This gives us access to the task ID and static server port. The extension then opens `http://127.0.0.1:{port}` in the browser.

Two compilation paths run in the LSP:

- **`do_phase0_only`** (triggered on every `didChange`): runs `compile_project_phase0_unsaved` with `Phase0Mode::Modified` — instant, no subprocess, shows amber borders on modified chunks. No generation guard (idempotent).
- **`do_compile`** (triggered on `didSave` or Run button): runs Phase 0 with `Phase0Mode::Pending` (orange), then full streaming compilation via `compile_project_full`. Protected by `compile_generation: Arc<AtomicU64>` — incremented on each `didSave` so stale in-flight compiles are discarded.

`TinymistOverlay { Inactive | Active { next_version } }` tracks whether `didOpen` has been sent to Tinymist. `apply_update(content, path, generation)` writes `.typ` to disk and sends `textDocument/didChange` to the Tinymist subprocess when the overlay is `Active`.

## Sync Mapping

`compiler/sync.rs` handles bidirectional source ↔ PDF navigation using `#KNOT-SYNC` markers embedded in the assembled `.typ`.

### CLI sync (both directions implemented)
- `knot jump-to-source <typ_file> <line>` — maps a `.typ` line → `.knot` file + line.
- `knot jump-to-typ <typ_file> <knot_file> <line>` — maps a `.knot` line → `.typ` line.

### LSP/VS Code sync
- **Forward sync** ✅ (`knot/syncForward`): cursor position in `.knot` editor → maps to `.typ` line → calls `tinymist.scrollPreview` on **our own** Tinymist subprocess. The key: using our subprocess (not the VS Code extension's) gives us the task ID and port.
- **Backward sync** ✅ (`window/showDocument`): click in PDF → Tinymist sends `window/showDocument` to our LSP → `handle_tinymist_show_document` maps `.typ` line → `.knot` line → VS Code opens the `.knot` file at the right position. The extension also handles auto-redirect when a `.typ` file is opened in the editor (via `knot jump-to-source`).

## Chunk Options

Options are written as YAML comments at the top of a chunk:
```
#| label: my-chunk
#| echo: false
#| fig-width: 6
#| freeze: [x, df]
```

Freeze objects: after a `freeze: [x]` chunk executes, Knot stores SHA-256 hashes of the named objects. After every subsequent `MustExecute` chunk in that language, `check_freeze_contract` verifies the hashes and cascades `Inert` on violation (`compiler/freeze.rs`).

## Configuration (`knot.toml`)

```toml
[document]
main = "main.knot"
includes = ["chapter1.knot"]

[execution]
timeout_secs = 30

[chunk-defaults]
echo = true

[r-chunks]
warning = false

[python-chunks]
# Python-specific defaults

[codly]
# Syntax highlighting config
```

## Key Conventions

- Error handling uses `anyhow::Result` everywhere; the `?` operator propagates freely.
- Parsing uses the **winnow** combinator library (`parser/winnow_parser.rs`).
- New chunk options must be added to `OptionMetadata` (drives both completion and docs).
- The `Show` enum (`Both | Code | Output | None`) controls what appears in the output `.typ`.
- `WarningsVisibility` (`Show | Hide | Above | Below`) controls where R/Python warnings appear.
- Graphics require environment variables to be set in the **child** process (not the parent Rust process).
