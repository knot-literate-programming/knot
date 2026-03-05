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
virtual `.typ` line numbers. This mapping is rebuilt on every compile.

For every LSP request that carries a position (definition, hover, completion,
formatting), `knot-lsp`:

1. Checks if the document is a `.knot` file.
2. Maps the `.knot` position to the corresponding `.typ` position.
3. Forwards the request to Tinymist with the translated position.
4. Maps the response positions back to `.knot` coordinates.

The mapping is exposed by `knot-core` via `compile_project_full`'s
`ProjectOutput.source_map`.

---

## Own handlers

These features are handled entirely by `knot-lsp` without forwarding to
Tinymist:

| Handler | File | What it does |
|---|---|---|
| Completion | `handlers/completion.rs` | `#\| ` triggers chunk-option completion |
| Hover | `handlers/hover.rs` | Hover over option names shows docs from `OptionMetadata` |
| Formatting | `handlers/formatting.rs` | Air (R) + Ruff (Python) + Tinymist (Typst) |
| Diagnostics | `diagnostics.rs` | Merges parse errors + runtime errors from cache |
| Symbols | `symbols.rs` | Document symbols for the `.knot` file |

---

## Preview lifecycle

### Starting the preview

`knot/startPreview` (a custom LSP method) triggers:

1. `compile_project_phase0` — instant, no code runs.
2. `apply_update` — sends `textDocument/didOpen` (v=1) to Tinymist.
3. `tinymist.doStartPreview` — starts a preview task on **our** Tinymist
   subprocess. This returns a `task_id` and a `static_server_port`.
4. The port is stored in `ServerState.preview_info`.
5. The response tells VS Code the URL to open: `http://127.0.0.1:{port}`.

> **Why use our own Tinymist subprocess?** The VS Code extension has its own
> Tinymist instance, but we cannot obtain its preview task ID. Our subprocess
> is under our control, so we can obtain both the task ID (needed for forward
> sync) and the static server port.

### Compilation on save

`did_save` in `server_impl.rs`:

1. Increments `compile_generation` (stale-guard).
2. Spawns `do_compile(generation)` in a background thread.

`do_compile`:

1. **Phase 0** (instant): `compile_project_phase0(Phase0Mode::Pending)` →
   orange placeholders → `apply_update` → Tinymist sees the `.typ` change.
2. **Streaming**: `compile_project_full(path, Some(callback))` — for each
   chunk that finishes, `apply_update` → Tinymist.
3. **Final**: last `apply_update` + `refresh_diagnostics`.

At every `apply_update` call, the generation is checked — if a newer `didSave`
has arrived, the in-flight compile is silently abandoned.

### Phase 0 on change

`did_change` triggers `do_phase0_only` (not `do_compile`):

1. `compile_project_phase0_unsaved(content, Phase0Mode::Modified)` — uses
   the in-memory buffer so the preview updates while the user types.
2. `apply_update` → Tinymist.

This is what produces the amber borders while typing: modified chunks are
rendered with state flags `is-modified` or `is-modified-cascade`, which the
`knot-state-styles` in `lib/knot.typ` renders as amber borders.

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
2. Maps the line back to a `.knot` file + line using `compile_project_full`'s
   source map.
3. Sends `window/showDocument` to VS Code with the `.knot` coordinates.

---

## Adding a new LSP feature

1. **If it needs Tinymist**: intercept the request in `proxy.rs`, map
   coordinates with `PositionMapper`, forward, map the response back.
2. **If it is Knot-specific**: add a handler in `handlers/`, register it
   in `server_impl.rs`'s request dispatch, and add any state to `ServerState`
   in `state.rs`.
3. **If it is a custom method** (like `knot/startPreview`): add a match arm
   in the custom-method dispatcher in `server_impl.rs`.
