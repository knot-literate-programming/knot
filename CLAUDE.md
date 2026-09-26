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

`knot build` and `knot watch` use `ProjectBuild::prepare` + `compile(None)` (non-streaming). VS Code preview uses `compile(Some(callback))` (streaming) so chunks appear as they complete.

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

**Pass 1 – Planning** (`pipeline.rs`): Parse the `.knot` document, resolve chunk options, compute SHA-256 hashes (chained sequentially so that editing chunk N invalidates N+1, N+2, …), and classify each node as `Skip`, `CacheHit`, `CacheHitInline`, `MustExecute` or `Rejected` (invalid options or missing `depends` file: not executed, chain suspended).

**Pass 2 – Execution** (`execution.rs`): Chunks tagged `MustExecute` are grouped by language via `group_by_language()`, then R and Python chains run **in parallel** via `std::thread::scope`. Within each language chain execution is sequential (preserving interpreter state). Results are written to cache.

**Pass 3 – Assembly** (`mod.rs`): Node outputs are interleaved with the source text to produce a `.typ` file consumed by Typst.

Key types:
```
ExecutionNeed        ::= Skip | CacheHit(ExecutionAttempt) | CacheHitInline(String) | MustExecute | Rejected
ExecutionAttempt     ::= Success(ExecutionOutput) | RuntimeError(RuntimeError)
ChunkExecutionState  ::= Ready            -- cache hit or just executed
                       | Inert            -- suspended due to upstream error
                       | Pending          -- Phase 0 on save/Run: orange border
                       | Modified         -- Phase 0 while typing: first MustExecute in chain (amber strong)
                       | ModifiedCascade  -- Phase 0 while typing: subsequent MustExecute in chain (amber muted)
Phase0Mode           ::= Pending | Modified | Blocked  -- Blocked: invalid YAML header, nothing runs
```

**Cascade Inert**: when a chunk in language L errors, all subsequent L-chunks become `Inert` (state is uncertain).

### Progressive Compilation

The compiler supports a two-phase API for live preview:

**Phase 0** (`plan_and_partial` in `mod.rs`): runs Pass 1 only — no code executed. Cache hits render with real output; `MustExecute` nodes render as visual placeholders controlled by `Phase0Mode`:
- `Phase0Mode::Pending` — compilation is in progress (save or Run): all pending chunks show orange border.
- `Phase0Mode::Modified` — user is editing without compiling (typing): first `MustExecute` per language chain shows amber (strong), subsequent ones amber (muted) to distinguish direct edits from hash-cascade invalidations.

**`ProjectBuild`** (`project/build.rs`) is the project-level API used by the CLI and the LSP:
- `ProjectBuild::prepare(path, buffers, cancellation)` — captures the configuration and every source (applying open buffers) and copies the committed caches into a private workspace; nothing shared is modified.
- `ProjectBuild::prepare_preview(...)` — same capture without the copy: reads the committed caches directly, for Phase 0 only (never executes). This keeps a keystroke at a few ms whatever the snapshot size (see `docs/dev-plans/perf-baseline.md`, `examples/bench_live.rs`).
- `build.phase0(mode)` — Phase 0 for the whole project.
- `build.compile(on_progress)` — full compilation; `on_progress` receives a complete `ProjectOutput` after each executed chunk (streaming).
- `build.publish(output, complete)` — writes `main.typ`; with `complete`, publishes the workspace caches and artifacts atomically.

`ProjectOutput` carries `errors` and `warnings` (`BuildDiagnostic`), which `knot build --strict` turns into a failing exit status. `compile_project_full` and `compile_project_phase0*` in `project.rs` are thin wrappers over `ProjectBuild`.

## Cache

`crates/knot-core/src/cache/` — SHA-256 addressed, persisted as `.knot_cache/metadata.json`. The hash of chunk N includes the hash of chunk N-1, so any change cascades invalidations forward.

**Snapshots** (`compiler/snapshot_manager.rs`, `resources/*/session.*`): after a chunk executes, the interpreter state can be saved (R: `save.image` + attached packages; Python: pickled `__main__`, re-imported modules, RNG state) so that a later cache miss restarts from the previous chunk instead of re-running the chain. They are on by default and can be disabled per document and language in the YAML header (`snapshots: {python: false}`). Validation is cheap on the hot path (`snapshot_is_valid`: size + mtime) and complete at restore (`snapshot_is_intact`: full hash). `knot build --no-snapshots` re-executes everything, and `--strict --no-snapshots` is the reproducibility check.

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
- Adds Knot-specific features directly: chunk-option completion (`handlers/completion.rs`), hover docs (`handlers/hover.rs`), document formatting (Air for R, Ruff for Python, embedded Typstyle for Typst via `handlers/formatting.rs`), diagnostics merging (`diagnostics.rs`), and document symbols (`symbols.rs`).
- Manages preview and sync via `knot/startPreview` and `knot/syncForward` custom LSP methods (see `sync.rs`).

`ServerState` uses `Arc<RwLock<>>` throughout for concurrent access.

### LSP Preview Architecture

`knot/startPreview` starts a Tinymist preview task **in our own Tinymist subprocess** (not the VS Code extension's). That subprocess runs the binary bundled with the Tinymist extension when the editor supplies it (`tools::resolve_editor_binary`: `[tools] tinymist`, then the editor's path, then `PATH`), so a stale `tinymist` on `PATH` cannot downgrade Typst in the preview. This gives us access to the task ID and static server port. The extension then opens `http://127.0.0.1:{port}` in the browser.

Compilation requests go through `compilation.rs`. Each project has one publication gate; `queue_compile` begins a new request, which cancels the previous one (its publications are discarded):

- **Typing** (`didChange`): debounced 300 ms, then `run_preview` — `ProjectBuild::prepare_preview` + `phase0(Phase0Mode::Modified)` under the gate. Instant, nothing executed, amber borders on modified chunks.
- **Save or Run** (`didSave`, Run button): `ProjectBuild::prepare`, Phase 0 with `Phase0Mode::Pending` (orange), then `build.compile` streaming each executed chunk, then final publication and diagnostics refresh.

`TinymistOverlay::Active { next_version }` (one entry per project; no entry means `didOpen` not sent yet) tracks the version of the generated `.typ` overlay; `send_overlay` sends `didOpen` or `textDocument/didChange` to our Tinymist subprocess.

## Sync Mapping

`compiler/sync.rs` handles bidirectional source ↔ PDF navigation using `#KNOT-SYNC` markers embedded in the assembled `.typ`.

### CLI sync (both directions implemented)
- `knot jump-to-source <typ_file> <line>` — maps a `.typ` line → `.knot` file + line.
- `knot jump-to-typ <typ_file> <knot_file> <line>` — maps a `.knot` line → `.typ` line.

### LSP/VS Code sync
- **Forward sync** ✅ (`knot/syncForward`): cursor position in `.knot` editor → maps to `.typ` line → calls `tinymist.scrollPreview` on **our own** Tinymist subprocess. The key: using our subprocess (not the VS Code extension's) gives us the task ID and port.
- **Backward sync** ✅ (`window/showDocument`): click in PDF → Tinymist sends `window/showDocument` to our LSP → `handle_tinymist_show_document` maps `.typ` line → `.knot` line → VS Code opens the `.knot` file at the right position. The extension also handles auto-redirect when a `.typ` file is opened in the editor (via `knot jump-to-source`).

## Chunk Options

Options are written as YAML comments at the top of a chunk; the label goes in the fence header:
~~~
```{r my-chunk}
#| show: output
#| fig-width: 6
#| caption: A figure
```
~~~

The reference table in `docs/book/src/chunk-options.md` is generated from `ChunkOptions::option_metadata()` (`cargo run -p knot-core --example chunk_options_reference`); `tests/docs_reference.rs` fails when it is stale.

## Configuration (`knot.toml`)

```toml
[document]
main = "main.knot"
includes = ["chapter1.knot"]

[execution]
timeout-secs = 30

[chunk-defaults]
show = "both"

[r-chunks]
warnings-visibility = "none"

[python-chunks]
# Python-specific defaults

[codly]
# Syntax highlighting config
```

## Two Distinct Styling Systems

Knot has two separate, intentionally asymmetric styling systems:

**1. Chunk presentation styles** — appear in the **final PDF**
- Configured via `knot.toml` (`[chunk-defaults]`, `[codly]`) or per-chunk options (`#| code-background:`, `#| code-stroke:`, etc.)
- Propagated by the Rust pipeline through `ResolvedChunkOptions` → `backend.rs` → `#code-chunk(...)` arguments
- Examples: code block background, border, inset, output layout

**2. Execution state styles** — appear **only in live preview**, never in the final PDF
- Configured directly in `knot-typst-package/lib.typ` (embedded in every compiled `.typ`) via the `knot-state-styles` dictionary
- Implemented purely in Typst; Rust only sets boolean flags (`is-pending`, `is-modified`, etc.)
- Examples: orange border for pending chunks, amber for modified, white overlay for inert

**Rule**: do not route state styles through `knot.toml` or the Rust config pipeline — `knot-typst-package/lib.typ` is the right and only place for them.

## Errors Are Visible in the PDF

**The PDF is the notebook.** Every error must be visible in the PDF — both the live preview and the final build — at the place it concerns, and reported by the LSP with the same message and a matching severity. An error that only reaches a log (`log::warn`), the CLI output or the editor is a bug.

- Render an error block rather than aborting the compilation: a document containing an error still produces a PDF that shows it.
- Produce the PDF and LSP messages from one shared helper in `knot-core` (e.g. `defaults::unsupported_language_message`).
- An execution error in the PDF is an error in the LSP, not a warning.
- Code marked `#| eval: false` is display-only: it is not an error.
- While typing, a chunk being edited (its text differs from the saved file) is shown as modified, without its option errors: the editor reports them at once, and the preview shows them on save (`Compiler::with_saved_source`). A chunk saved with invalid options keeps its error block.
- Runtime text (messages, code, outputs) must be escaped with the helpers in `typst_syntax.rs`, never inserted as raw markup.
- Messages are plain text in both the PDF and the editor: quote literals with single quotes ('#| eval: false', 'knot clean'), never with Markdown backticks.
- Document-level warnings (configuration, snapshot budget) go at the end of the document: content placed before a template's page rules would add a page.
- Generated Typst must not depend on packages the user may not import (e.g. codly's `local()`), except for options the user explicitly configured.

Open gaps are tracked in issue #96.

## Key Conventions

- Error handling uses `anyhow::Result` everywhere; the `?` operator propagates freely.
- Parsing uses the **winnow** combinator library (`parser/winnow_parser.rs`).
- New chunk options must be added to `OptionMetadata` (drives both completion and docs).
- The `Show` enum (`Both | Code | Output | None`) controls what appears in the output `.typ`.
- `WarningsVisibility` (`Below | Inline | None`) controls where R/Python warnings appear.
- Graphics require environment variables to be set in the **child** process (not the parent Rust process).
