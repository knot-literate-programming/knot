# The Language Server

`knot-lsp` is a **proxy LSP server**. It sits between VS Code and Tinymist
(the official Typst Language Server), intercepting requests, translating
coordinates, and injecting Knot-specific features.

---

## Architecture

```
[VS Code]
    │ LSP (stdio)
    ▼
[knot-lsp]
    ├── Own handlers (completion, hover, formatting, diagnostics, preview)
    │
    └── [Tinymist subprocess]
            │ LSP (stdio, internal)
            ▼
          Typst analysis, symbol resolution, preview rendering
```

The key insight: Knot compiles `.knot` → virtual `.typ`. The editor works on
`.knot` files, but Tinymist only understands `.typ`. The LSP bridges this gap.

---

## Coordinate translation

`position_mapper.rs` maintains a mapping between `.knot` line numbers and
virtual `.typ` line numbers. This mapping is rebuilt on every buffer update.

For Typst requests forwarded to Tinymist (for example hover and completion),
`knot-lsp`:

1. Checks if the document is a `.knot` file.
2. Maps the `.knot` position to the corresponding `.typ` position.
3. Forwards the request to Tinymist with the translated position.
4. Maps the response positions back to `.knot` coordinates.

The generated document also contains `BEGIN-FILE` and `KNOT-SYNC` markers used
for navigation between assembled output and source files.

---

## Own handlers

These features are handled entirely by `knot-lsp` without forwarding to
Tinymist:

| Handler | File | What it does |
|---|---|---|
| Completion | `handlers/completion.rs` | `#\| ` triggers chunk-option completion |
| Hover | `handlers/hover.rs` | Hover over option names shows docs from `OptionMetadata` |
| Formatting | `handlers/formatting.rs` | Shared `knot-core::formatting` engine: Air + Ruff + embedded Typstyle |
| Diagnostics | `diagnostics.rs` | Merges parse errors + runtime errors from cache |
| Symbols | `symbols.rs` | Document symbols for the `.knot` file |

---

## Preview lifecycle

### Project coordination

`compilation.rs` owns one generation counter, cancellation token and publication
mutex per canonical project root. The main document and its includes share that
coordinator; different projects remain independent.

Typing registers a new generation immediately. Only the Phase-0 work is debounced
(300 ms), so a running save/Run request is invalidated before the debounce expires.
Save and `knot/compile` use the same pipeline. Closing a buffer renders disk state;
cleaning the project also invalidates pending work. Shutdown cancels all projects.

A request captures all open buffers in its project and their LSP versions.
`ProjectBuild::prepare` reads configuration and remaining sources once and seeds a
private cache under `.knot_cache/.build-*`. Preparation and publication share the
project mutex, so the seed cannot observe a partially published cache. Interpreter
execution runs outside that mutex. Included documents use the same source snapshot
as the main document.

### Publication

`ProjectBuild` separates computation from publication:

1. Phase 0 renders cached results and pending/modified placeholders privately.
2. Full compilation executes includes, then streams completed main-file chunks.
   Intermediate output does not commit cache metadata.
3. Final publication commits artifacts and caches, then atomically replaces the
   generated Typst file. Metadata is copied after the files it references.

For every publication, the LSP holds the project mutex and checks the generation
before touching shared files, sending a Tinymist overlay, refreshing Knot runtime
diagnostics, or sending completion. A newer generation cannot register midway
through these actions. Old workers finish in private storage and cannot publish.
Runtime diagnostics are also checked against the captured document version.

`knot/compilationStarted`, `knot/compilationInvalidated` and
`knot/compilationComplete` carry `uri`, `project`, `generation` and `versions`;
completion also carries `success`. The extension checks generations and its own
current buffer versions, including includes, before displaying “Up to date”.
Failed preparation/publication ends the current spinner with a failure status.

Cancellation is cooperative: an interpreter call already running finishes or
reaches its configured timeout. Subsequent chunks are skipped, and workers are
joined before their private workspace is removed. Knot cannot undo arbitrary
user-code side effects such as writing a data file or making a network request.

The mutex coordinates **one LSP process**. Independent CLI processes, including
VS Code's separate Build PDF command, must not write to the same project at the
same time. Cross-process locking is not implemented. Publication uses atomic
replacement per file, not a crash-atomic transaction across every cache file.

### Cost of isolation

Private cache copies add disk I/O and temporary storage. A local release-mode
probe on Apple Silicon (three warm runs, Python bytearray state saved in two
snapshots) measured the following medians:

| Total committed cache | Direct compiler planning | Prepare + Phase 0 + publication |
|---|---:|---:|
| About 2 KiB | < 1 ms | about 1 ms |
| 32 MiB | 98 ms | 125 ms |
| 128 MiB | 394 ms | 509 ms |

The preparation/copy portion was about 26 ms and 114 ms for the larger cases.
Snapshot integrity checking already accounts for most of the planning time.
These are an indicative local microbenchmark, not an interactive latency budget:
they exclude the 300 ms debounce, Tinymist rendering, concurrent requests, and
final interpreter-session synchronization. Large projects merit further profiling
before optimizing cache copies or snapshot validation.

### Starting the preview

`knot/startPreview` serializes startup for that project. Under the publication
mutex it opens the generated Typst overlay (or renders Phase 0 when no output
exists). It releases the mutex before waiting for `tinymist.doStartPreview`.
The response includes a project-specific `taskId` and `staticServerPort`; the
extension opens that task in the browser. Further starts reuse the project port.

Each generated document has its own increasing overlay version. Startup never
replays captured content after waiting for Tinymist: a newer compilation may
already have published during that wait.

### Diagnostics from Tinymist

The proxy advertises `publishDiagnostics.versionSupport`; versioned diagnostics
are discarded when they do not match the current source buffer. Tinymist 0.15.2
still sends unversioned diagnostics and does not implement diagnostic pull requests
(verified locally). These remain accepted for compatibility, so they cannot carry
the same freshness guarantee as Knot runtime diagnostics. Stronger correlation of
upstream diagnostics remains part of subsequent LSP work.

---

## Sync

### Forward sync (source → PDF)

`knot/syncForward` receives the cursor's `.knot` line, maps it to a `.typ`
line via `PositionMapper`, then calls `tinymist.scrollPreview` on our
subprocess with the `task_id` from `preview_info`.

### Backward sync (PDF → source)

Tinymist sends a `window/showDocument` notification when the user clicks in
the PDF. `handle_tinymist_show_document` in `server_impl.rs`:

1. Receives the `.typ` file path + line.
2. Maps the line back to a `.knot` file + line using the generated source markers.
3. Sends `window/showDocument` to VS Code with the `.knot` coordinates.

---

## Adding a new LSP feature

1. **If it needs Tinymist**: intercept the request in `proxy.rs`, map
   coordinates with `PositionMapper`, forward, map the response back.
2. **If it is Knot-specific**: add a handler in `handlers/`, register it
   in `server_impl.rs`'s request dispatch, and add any state to `ServerState`
   in `state.rs`.
3. **If it is a custom method** (like `knot/startPreview`): register it with
   `LspService::build` in `main.rs`.

## Shared document formatting

The CLI and LSP call `knot_core::formatting::format_document`. The LSP runs it on a
blocking worker and checks the source version before returning a whole-document
edit. It no longer opens or updates a Tinymist overlay for formatting.

The core validates Knot blocks, formats their code, and substitutes unique opaque
raw placeholders before invoking the pinned Typstyle library. Reconstruction
checks placeholder identity, uniqueness and order; any mismatch is an error.
This formatting representation is separate from the virtual document used for
hover, completion and diagnostics, whose positions must remain aligned.
