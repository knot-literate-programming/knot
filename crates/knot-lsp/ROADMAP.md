# Knot LSP Roadmap

This document outlines the technical roadmap for `knot-lsp`, aiming to transition from a functional prototype to a robust, production-ready language server.

## Phase 1: Core Reliability & Diagnostics (Critical)

The immediate priority is to ensure the user sees errors (Typst syntax & Knot structure) and that the communication layer is stable.

- [ ] **Bi-directional Diagnostics Tunneling**
  - [ ] Implement a background task to consume `tinymist` notifications (`textDocument/publishDiagnostics`).
  - [ ] Map Typst error ranges back to Knot file positions using `PositionMapper`.
  - [ ] Merge Typst diagnostics with local Knot diagnostics (parsing errors).
  - [ ] Publish the unified diagnostic list to the LSP client.

- [ ] **Robust LSP Transport Layer**
  - [ ] Refactor `proxy.rs` to replace manual header parsing.
  - [ ] *Option A:* Adopt `lsp-server` crate for handling the JSON-RPC wire protocol.
  - [ ] *Option B:* Use `tower-lsp`'s lower-level primitives if applicable.
  - [ ] Ensure correct handling of `Content-Length` and UTF-8 boundaries to prevent de-sync with `tinymist`.

## Phase 2: Standard LSP Features (Navigation)

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
  - [ ] Improvement: Ensure Tinymist markdown content is correctly offset/mapped if it refers to code lines.

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
  - [ ] Add configuration for Python formatter path.

- [ ] **Signature Help**
  - [ ] R/Python: Use `executors` to fetch function signatures during typing.
  - [ ] Typst: Forward to `tinymist`.

- [ ] **Variable Explorer (Custom Command)**
  - [ ] Implement a custom LSP command (e.g., `knot/getVariables`).
  - [ ] Query active R/Python sessions for variables/dataframes.
  - [ ] Return structured JSON for the VSCode extension to display in a side panel.

## Phase 4: Architecture & Maintenance

- [ ] **Error Handling & Logging**
  - [ ] expose `tinymist` stderr output to the Client's Output Channel for easier debugging.
  - [ ] Graceful degradation: If `tinymist` crashes, R/Python features should still work.

- [ ] **Testing**
  - [ ] Add integration tests simulating LSP exchanges (mocking the client).
  - [ ] Create a "corpus" of edge-case Knot files (mixed indentation, weird chunk boundaries) to test the `PositionMapper`.
