# Knot LSP Roadmap

This document outlines the technical roadmap for `knot-lsp`, aiming to transition from a functional prototype to a robust, production-ready language server.

## Phase 1: Core Reliability & Diagnostics (DONE ✅)

The immediate priority was to ensure the user sees errors (Typst syntax & Knot structure) and that the communication layer is stable.

- [x] **Bi-directional Diagnostics Tunneling**
  - [x] Implement a background task to consume `tinymist` notifications (`textDocument/publishDiagnostics`).
  - [x] Map Typst error ranges back to Knot file positions using `PositionMapper` (via virtual `.knot.typ` trick).
  - [x] Merge Typst diagnostics with local Knot diagnostics (parsing errors).
  - [x] Publish the unified diagnostic list to the LSP client.
- [x] **Robust LSP Transport Layer**
  - [x] Refactor `proxy.rs` to replace manual header parsing with `tokio-util` and `LspCodec`.
  - [x] Ensure correct handling of `Content-Length` and UTF-8 boundaries to prevent de-sync with `tinymist`.
- [x] **Precise Knot Diagnostics**
  - [x] Implement precise line-offset reporting for chunk options errors.
  - [x] Standardize chunk options parsing using YAML engine.

## Phase 2: Standard LSP Features (Next Priority 🚧)

Implement standard editor capabilities by acting as a smart proxy between the client and `tinymist`.

- [ ] **Go to Definition / Declaration**
  - [ ] Intercept `textDocument/definition`.
  - [ ] Map input position (Knot -> Typst).
  - [ ] Forward request to `tinymist`.
  - [ ] Map response locations (Typst -> Knot).
- [ ] **References & Rename**
  - [ ] Implement `textDocument/references` (with position mapping).
  - [ ] Implement `textDocument/rename` (complex: requires verifying that renames don't touch generated Knot structures).
- [ ] **Enhanced Hover**
  - [ ] Current: Handles R/Python dynamically, text via Tinymist.
  - [ ] Improvement: Ensure Tinymist markdown content is correctly mapped.
- [ ] **Document Symbols**
  - [ ] Merge local symbols (Chunks) with Tinymist symbols (Headings, Functions).
  - [ ] Provide a unified outline view.

## Phase 3: The "Knot" Experience (Polyglot Features)

Features specific to the multi-language nature of Knot.

- [ ] **Hybrid Formatting**
  - [ ] Strategy: 
    1. Send document to `tinymist` for text formatting.
    2. Extract text edits, filter out those touching chunks.
    3. Format chunks locally using `Air` (R) or `Black` (Python).
    4. Merge edits intelligently.
- [x] **Python/R Parity (Core Engine)**
  - [x] Harmonize Python executor structure with R.
  - [x] Implement hash verification for constant loading in Python.
  - [x] Implement smart inline formatting for Python (scalars vs collections).
- [ ] **Signature Help**
  - [ ] R/Python: Use `executors` to fetch function signatures during typing.
  - [ ] Typst: Forward to `tinymist`.
- [ ] **Variable Explorer (Custom Command)**
  - [ ] Implement a custom LSP command (e.g., `knot/getVariables`).
  - [ ] Return structured JSON for the VSCode extension to display in a side panel.

## Phase 4: Architecture & Maintenance

- [ ] **Error Handling & Logging**
  - [ ] Expose `tinymist` stderr output to the Client's Output Channel for easier debugging.
- [x] **CI/CD Stabilization**
  - [x] Fix race conditions in environment variables during tests.
  - [x] Automate releases via GitHub Actions (`cargo-dist`).
- [ ] **Testing**
  - [ ] Add integration tests simulating LSP exchanges.