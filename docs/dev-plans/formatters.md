# Code Formatters Integration (Hybrid Strategy)

**Goal:** Provide professional-grade, unified code formatting for Typst, R, and Python within Knot documents.

## 🛠️ Tool Choices

- **R: Air**: Written in Rust, extremely fast. Integrated via temporary file for consistent CLI/LSP behavior.
- **Python: Ruff**: Industry standard, integrated via stdin/stdout for performance.
- **Typst: Tinymist (Proxy)**: Handled by the LSP proxy for global document formatting.
- **Knot Core: Structural Normalizer**: Internal logic to clean chunk headers and YAML options.

## ✅ Implementation Status

### 1. Hybrid Formatting Pipeline
- [x] **Step A: Internal Code Formatting**: Integration with `air` and `ruff`.
- [x] **Step B: Structural Normalization**: Normalizing chunk headers (e.g., ````{r  name }`` -> ````{r name}``) and YAML options formatting (`#|key:val` -> `#| key: val`).
- [x] **Step C: CLI Integration**: `knot format` command available for bulk processing.
- [x] **Step D: LSP Integration**: Support for both `textDocument/formatting` (full document) and `knot/formatChunk` (selection/command).

### 2. VS Code Integration
- [x] **Format Chunk Command**: Dedicated command to format the chunk under the cursor.
- [x] **Non-blocking Activation**: Commands are registered immediately, and LSP/Tinymist start in the background to ensure UI responsiveness.
- [x] **Robust Communication**: Custom LSP request `knot/formatChunk` used to bypass standard command argument limitations.

### 3. Full-Document Formatting — 3-Phase Pipeline (`feat/full-document-formatting`)

The LSP `textDocument/formatting` handler now runs a full 3-phase pipeline:

**Phase A — Code formatting (Air / Ruff)**
- `CodeFormatter` consolidated in `knot-core` (single source of truth, sync, no tokio dependency).
- LSP wraps with `spawn_blocking` to avoid blocking the async runtime.
- Binary paths (`airPath`, `ruffPath`) are read from `initializationOptions` and stored in `ServerState`.
- Formatter unavailable → Phase A skipped gracefully (structural normalization still applied).

**Phase B — Typst formatting (Tinymist) — Mirror Mask strategy**
- The `.knot` document is transformed into a `.typ` mask: fence headers preserved, code bodies replaced by blank lines (line count maintained for position fidelity).
- The mask is sent to Tinymist under a virtual URI, and its formatting edits are applied.
- Tinymist unavailable → Phase B skipped gracefully, Phase A result flows to Phase C.

**Phase C — Document reconstruction**
- The formatted Typst structure is parsed to locate each chunk/inline by byte position.
- Three validation guards prevent silent corruption:
  1. Element count parity (chunks + inlines)
  2. Language correspondence (pairwise, per chunk)
  3. Non-overlapping element ranges (panic guard)
- On any guard failure: fallback to Phase A output with a `log::warn!`.
- Indentation detected from Tinymist output and forwarded to `Chunk::format()` without mutating the AST.

**Code quality fixes landed alongside (fixes #1–#7):**
- `ServerState` derives `Clone`; `clone_for_task()` simplified.
- `CodeFormatter` (Air + Ruff) consolidated in `knot-core`; removed duplicate LSP implementation.
- Opaque `(usize, usize, bool, usize)` tuple replaced by `Element` / `ElementKind` enum.
- `try_load_snapshot()` extracted to deduplicate `sync_with_cache()`.
- All silent fallbacks now emit `log::warn!` / `log::debug!`.
- LSP method magic strings replaced by typed constants in `lsp_methods.rs`.
- 22 unit + integration tests covering happy paths, fallbacks, UTF-16 edge cases.

## 📋 Technical Details: Structural Normalization

Beyond external tools, Knot performs "Structural Normalization" to ensure consistency:

- **Header Cleaning**: Extra spaces in ` ```{lang name} ` are stripped.
- **Options Firewall**: Unknown options (except `codly-*`) trigger diagnostics and are preserved but highlighted.
- **YAML Formatting**: Options are strictly formatted as `#| key: value`.
- **Spacing**: A blank line is automatically maintained between the last option and the first line of code if needed.

## ⚠️ Known Limitations & Future Work

### Error surfacing to the user
The current pipeline fails silently (falls back, logs via `log::warn!`) but never notifies the user through the editor UI. Two distinct mechanisms are needed:

- **Tinymist failures**: Tinymist is an LSP and already emits `window/showMessage` / `window/logMessage` notifications. Our `handle_tinymist_notification()` currently drops them. Forwarding them to `self.client` would be a small, targeted fix.
- **Air / Ruff failures**: These are CLI tools — they have no notification mechanism. When they return a non-zero exit code, *we* must generate a `window/showMessage`. Requires either passing `self.client` to `handle_formatting()` or returning error info to the caller. UX consideration: notifications should fire **at most once** (e.g., on startup if the binary is missing), not on every save.

### Virtual document version tracking (`version + 1000` hack)

`handle_formatting` always sends `textDocument/didOpen` to Tinymist for the virtual URI, even on the 2nd, 3rd, ... call. The `version + 1000` offset was added to avoid Tinymist rejecting a repeated `didOpen` with the same version. This is incorrect LSP protocol.

**Clean fix**: add `virtual_version: Option<i32>` to `DocumentState` and mirror the existing `opened_in_tinymist` / `forward_to_tinymist` pattern:
- `None` → send `didOpen` with version 1
- `Some(v)` → send `didChange` with version `v + 1`

Payload structure differs between the two methods (same as `forward_to_tinymist` in `main.rs`). `virtual_version` must be reset to `None` in `did_close` so that a Tinymist restart doesn't receive a stale `didChange`.

### Missing end-to-end integration test
No test covers all three phases simultaneously (requires a live Air + Tinymist process).

### Previously noted
- **Indentation Preservation**: Python chunks inside deeply nested Typst structures maintain correct relative indentation (resolved by passing indentation explicitly to `Chunk::format()`).
- **Configuration**: Air and Ruff binary paths now configurable via VS Code settings (`knot.formatter.air.path`, `knot.formatter.ruff.path`) and forwarded through `initializationOptions`.

## 🎯 Success Criteria
- [x] Messy R code (`x<-1+1`) is cleaned to `x <- 1 + 1`.
- [x] Python docstrings and indentation are standardized via Ruff.
- [x] Typst headings and whitespace are normalized via Tinymist.
- [x] Formatting is idempotent.
- [x] **Performance**: Formatting remains sub-200ms for typical documents.
- [x] **Resilience**: Any phase failure falls back gracefully without data loss.
