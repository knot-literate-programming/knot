# Architecture Overview

Knot is structured as a Rust workspace with four components:

```
knot/
├── crates/
│   ├── knot-core/     # Engine: parser, compiler, cache, executors
│   ├── knot-cli/      # knot command-line tool
│   └── knot-lsp/      # Language Server (Tinymist proxy + Knot overlays)
└── editors/
    └── vscode/        # VS Code extension (TypeScript)
```

## Component responsibilities

### knot-core

The heart of the system. Everything that makes Knot work lives here:

- **Parser** (`parser/`): Winnow combinator parser that turns `.knot` files into an
  AST of `Node` values — prose, code chunks, and inline expressions.
- **Compiler** (`compiler/`): Three-pass pipeline (plan → execute → assemble).
- **Cache** (`cache/`): SHA-256 addressed persistent cache in `.knot_cache/`.
- **Executors** (`executors/`): Persistent R and Python subprocesses.
- **Backend** (`backend.rs`): Renders `Node`s into `.typ` text.
- **Project** (`project.rs`): Top-level API — `compile_project_full`,
  `compile_project_phase0`, etc.

`knot-core` has no tokio dependency. Concurrency is `std::thread::scope`.

### knot-lsp

An LSP server that wraps Tinymist (the official Typst LSP) and adds Knot-specific
capabilities:

- Forwards most LSP requests to a Tinymist subprocess after mapping `.knot`
  coordinates to virtual `.typ` coordinates.
- Adds chunk-option completion, hover docs, hybrid formatting, and diagnostics.
- Manages streaming preview via `knot/startPreview` and `knot/syncForward`.

### knot-cli

A thin binary over `knot-core`. Most commands delegate directly to project-level
functions (`compile_project_full`, etc.).

### editors/vscode

A VS Code extension written in TypeScript. It communicates with `knot-lsp` via the
Language Server Protocol and adds editor UI (status bar, preview lifecycle,
auto-redirect from `.typ` to `.knot`).

---

## Data flow: a save event

Here is the full path a `didSave` event takes from VS Code to a rendered PDF:

```
[VS Code]
  didSave
    │
    ▼
[knot-lsp — server_impl.rs::did_save]
  ├── increment compile_generation
  ├── spawn do_compile(generation)
  │
  └── [do_compile]
        ├── Phase 0: compile_project_phase0 (instant, orange placeholders)
        │     └── apply_update → textDocument/didChange → Tinymist
        │
        ├── Streaming: compile_project_full(path, Some(callback))
        │     └── for each chunk executed:
        │           apply_update → textDocument/didChange → Tinymist
        │
        └── Final: apply_update + refresh_diagnostics
                 → textDocument/didChange → Tinymist
                 → publishDiagnostics → VS Code

[Tinymist subprocess]
  textDocument/didChange
    → recompile .typ
    → push updated PDF to browser preview
```

---

## Key types

### In knot-core

```rust
// A parsed document node
enum Node {
    Prose(String),
    CodeChunk { language, options, code, … },
    Inline { language, expr, … },
}

// How much of the chunk's output to include
enum Show { Both, Code, Output, None }

// What execution work is needed
enum ExecutionNeed {
    Skip,                        // eval: false
    CacheHit(ExecutionAttempt),  // hash matched cache
    MustExecute,                 // must re-run
}

// The result of running (or attempting to run) a chunk
enum ExecutionAttempt {
    Success(ExecutionOutput),
    RuntimeError(RuntimeError),
}

// Visual state used in .typ output
enum ChunkExecutionState {
    Ready,           // cache hit or just executed
    Inert,           // suspended: upstream error
    Pending,         // compilation in progress (orange)
    Modified,        // direct edit, pre-save (amber, thick)
    ModifiedCascade, // hash-cascade, pre-save (amber, thin)
}
```

### In knot-lsp

```rust
// Whether Tinymist has received didOpen for the virtual .typ
enum TinymistOverlay {
    Inactive,
    Active { next_version: u64 },
}
```
