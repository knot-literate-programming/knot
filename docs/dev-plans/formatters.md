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

**Phase A — Code formatting (Air / Ruff) & Structural Normalization**
- `CodeFormatter` consolidated in `knot-core` (single source of truth).
- `Document::format` in `knot-core` provides a unified entry point for both CLI and LSP.
- LSP uses a 2-step async/sync process: chunks are formatted in parallel via `spawn_blocking`, then the document is reconstructed using the pure `Document::format` logic.
- Binary paths (`airPath`, `ruffPath`) are read from `initializationOptions` and stored in `ServerState`.
- Formatter unavailable/fails → Phase A continues with structural normalization and notifies the user via `window/showMessage` (once per session).

**Phase B — Typst formatting (Tinymist) — Mirror Mask strategy**
- The `.knot` document is transformed into a `.typ` mask: fence headers preserved, code bodies replaced by blank lines (line count maintained for position fidelity).
- The mask is sent to Tinymist under a virtual URI with proper version tracking (`virtual_version` in `DocumentState`).
- Tinymist unavailable → Phase B skipped gracefully, Phase A result flows to Phase C.

**Phase C — Document reconstruction**
- The formatted Typst structure is parsed to locate each chunk/inline by byte position and index.
- Three validation guards prevent silent corruption:
  1. Element count parity (chunks + inlines)
  2. Language correspondence (pairwise, per chunk)
  3. Non-overlapping element ranges (panic guard)
- On any guard failure: fallback to Phase A output with a `log::warn!`.
- Indentation detected from Tinymist output and forwarded to `Chunk::format()` without mutating the AST.

## 📋 Technical Details: Structural Normalization

Beyond external tools, Knot performs "Structural Normalization" to ensure consistency:

- **Header Cleaning**: Extra spaces in ` ```{lang name} ` are stripped.
- **Options Firewall**: Unknown options (except `codly-*`) trigger diagnostics and are preserved but highlighted.
- **YAML Formatting**: Options are strictly formatted as `#| key: value`.
- **Spacing**: A blank line is automatically maintained between the last option and the first line of code if needed.
- **Inline Normalization**: ` `{lang} code ` is normalized to a standard spacing.

## ⚠️ Known Limitations & Future Work

### Error surfacing to the user
- [x] **Air / Ruff failures**: The user is now notified via `window/showMessage` when a formatter returns an error.
- [x] **Tinymist failures**: Tinymist `window/showMessage` notifications are forwarded to the client (visible toast); `window/logMessage` notifications are forwarded to the output panel. Both are prefixed `[Tinymist]` for easy identification. Implemented in `handle_tinymist_notification` (`knot-lsp/src/main.rs`).

### Integration Testing
- [ ] **End-to-end integration test**: No test covers all three phases simultaneously (requires a live Air + Tinymist process).

## 🎯 Success Criteria
- [x] Messy R code (`x<-1+1`) is cleaned to `x <- 1 + 1`.
- [x] Python docstrings and indentation are standardized via Ruff.
- [x] Typst headings and whitespace are normalized via Tinymist.
- [x] Formatting is idempotent.
- [x] **Performance**: Formatting remains sub-200ms for typical documents due to parallel chunk formatting and minimal locking.
- [x] **Resilience**: Any phase failure falls back gracefully without data loss.
