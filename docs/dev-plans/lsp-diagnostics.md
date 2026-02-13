# LSP Diagnostics — Runtime Warnings & Errors

**Status:** 📋 Planned
**Depends on:** `feat/r-error-handling` merged

## Goal

Surface R/Python runtime warnings and errors as LSP diagnostics (squiggles) in the editor,
at the chunk level (not line-level — see Constraints below).

## Current state

Warnings and errors from `knot build` are already:
- Captured during execution (side-channel)
- Stored in `ChunkCacheEntry.warnings` (cache metadata)
- Rendered in the Typst document output

They are NOT yet sent to the editor as LSP diagnostics.

## Implementation plan

### 1. Surface warnings from cache (after build)

In `sync_with_cache()` (`crates/knot-lsp/src/main.rs`), after loading the session snapshot:

1. Read `metadata.json` from the cache directory
2. For each `ChunkCacheEntry` with non-empty `warnings`:
   - Find the chunk span in the parsed document (`parse_document(text)`)
   - Create a `Diagnostic` with `severity: Warning`, pointing to the chunk's opening fence line
3. Store in `knot_diagnostics_cache` and call `publish_combined_diagnostics()`

### 2. Surface errors from cache (after failed build)

Errors currently `bail!()` without being stored in cache. To surface them:

1. Add `error: Option<RuntimeError>` to `ChunkCacheEntry`
2. In `execute()`, write a partial cache entry before `bail!()`
3. Same LSP flow as warnings, with `severity: Error`

### 3. R syntax diagnostics (live, no execution)

The background R executor can validate syntax in real-time using `query()`:

```r
tryCatch(parse(text = CODE), error = function(e) e$message)
```

This does not touch the environment. Call on `did_change` for each R chunk.
Returns error message with line/column info from R's parser, allowing finer-grained
diagnostics within the chunk.

## Constraints

### No line-level resolution for runtime warnings

R's `withCallingHandlers` provides `w$call` (the function call site) but not a line number
within the chunk. `RuntimeWarning.line` is reserved but never populated. Chunk-level
granularity (highlighting the opening fence) is the realistic target.

### 1:1 position mapping invariant

The `.knot` ↔ `.typ` position mapping is identity-based: non-code content is replaced
with spaces/newlines (same line count). This is the same strategy as otter.nvim.
**Any change to the virtual `.typ` generation must preserve this line count invariant.**

## What Quarto does (for reference)

- **Static diagnostics**: via otter.nvim, which creates hidden language-specific buffers
  with blank lines for non-code content (same 1:1 strategy). The R/Python LSP attaches
  to these buffers for linting.
- **Runtime errors**: shown in the terminal only (`Quitting from lines 7-10 [chunk-name]`).
  Not surfaced as LSP diagnostics.

knot's approach (chunk-level runtime diagnostics from cache) would be an improvement
over what Quarto currently does.
