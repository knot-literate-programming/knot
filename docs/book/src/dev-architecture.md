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
- **Project** (`project.rs`, `project/build.rs`): Top-level API —
  `ProjectBuild` (isolated build workspace, atomic publication) and thin
  wrappers such as `compile_project_full`.

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
functions (`ProjectBuild`, etc.).

### editors/vscode

A VS Code extension written in TypeScript. It communicates with `knot-lsp` via the
Language Server Protocol and adds editor UI (status bar, preview lifecycle,
auto-redirect from `.typ` to `.knot`).

---

## Data flow: a save event

Here is the full path a `didSave` event takes from VS Code to a rendered PDF
(`crates/knot-lsp/src/compilation.rs`):

```
[VS Code]
  didSave
    │
    ▼
[knot-lsp — queue_compile(full)]
  ├── begin a new request for the project: cancels the previous one
  │   (typing, save or Run) and invalidates its publications
  │
  └── [run_compilation]
        ├── ProjectBuild::prepare — isolated workspace with the open buffers
        │
        ├── Phase 0: build.phase0(Pending) (instant, orange placeholders)
        │     └── publish → textDocument/didChange → Tinymist
        │
        ├── Streaming: build.compile(Some(callback))
        │     └── for each chunk executed:
        │           publish → textDocument/didChange → Tinymist
        │
        └── Final: build.publish(complete) + refresh diagnostics
                 → textDocument/didChange → Tinymist
                 → publishDiagnostics → VS Code

[Tinymist subprocess]
  textDocument/didChange
    → recompile .typ
    → push updated PDF to browser preview
```

While typing (`didChange`), `queue_compile` waits 300 ms (each keystroke
cancels the previous request), then `run_preview` renders Phase 0 in
`Modified` mode with `ProjectBuild::prepare_preview`, which reads the committed
caches without copying them. Nothing is executed. All publications of a
project go through one gate, and only the current request may publish.

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
    CacheHit(ExecutionAttempt),  // hash matched cache
    CacheHitInline(String),      // inline expression found in cache
    MustExecute,                 // must re-run
    Skip,                        // eval: false
    Rejected,                    // invalid options or missing depends file
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
// Per project: didChange version of the generated .typ overlay in Tinymist
// (no entry: didOpen not sent yet)
enum TinymistOverlay {
    Active { next_version: u64 },
}
```
