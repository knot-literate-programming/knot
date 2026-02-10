# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project. Detailed designs for specific features are located in their respective files within this directory.

## 🏗️ Project Overview

- **Core**: The engine (parsing, execution, cache).
- **LSP**: Language Server for IDE support (VS Code).
- **CLI**: Command-line interface for batch processing and watch mode.

---

## 🎯 Current Priorities

### 1. Robustness & Error Handling (Core + LSP)
Ensure that errors from R/Python are not just captured, but reported precisely in the editor.
- [ ] **R Error Handling**: Implement structured detection. See [r-error-handling.md](r-error-handling.md).
- [ ] **Python Error Handling**: Reach parity with R. See [python-error-handling.md](python-error-handling.md).
- [ ] **Live Diagnostics**: Feed executor errors back to the LSP for real-time red underlines.

### 2. Standard IDE Navigation (LSP)
Make Knot feel like a native editor for both Typst and the embedded languages.
- [ ] **Navigation**: Go to Definition, Hover, and References. See [lsp-navigation.md](lsp-navigation.md).
- [ ] **Completion**: Enhance Typst and multi-language autocompletion.

### 3. Integrated Workflow (Sync Mapping)
Close the loop between the source code and the generated PDF.
- [ ] **Sync Mapping**: Bidirectional click (Source ↔ PDF). See [sync-mapping.md](sync-mapping.md).

---

## 📅 Roadmap by Component

### Knot Core
- [x] YAML-based options parsing.
- [x] Macro-based options definition.
- [ ] Implement structured error handlers for all executors.
- [x] **Chunk display customization**: Flexible presentation options. See [chunk-customization.md](chunk-customization.md).
- [ ] Support for Julia executor.
- [ ] Support for Content-Generators (Mermaid, LilyPond).

### Knot LSP
- [x] Bi-directional diagnostics tunnel for Typst.
- [x] Precise line-offset for Knot structure errors.
- [ ] **Phase 2**: Navigation features (Hover, Definition).
- [ ] **Phase 3**: Hybrid Formatting (Air/Ruff + Tinymist).
- [ ] **Phase 4**: Variable Explorer (Dynamic introspection).

### Knot CLI
- [x] Watch mode.
- [ ] Improved error logging for CI/CD.
- [x] Built-in project initialization (`knot init`).

---

## 🔗 Design Documents
- [r-error-handling.md](r-error-handling.md): Detailed plan for R exection errors.
- [python-error-handling.md](python-error-handling.md): Parity plan for Python errors.
- [sync-mapping.md](sync-mapping.md): PDF-to-Source synchronization plan.
- [lsp-navigation.md](lsp-navigation.md): Roadmap for navigation features.
- [formatters.md](formatters.md): Plan for Air (R) and Ruff (Python) integration.
- [chunk-customization.md](chunk-customization.md): Flexible chunk presentation options.
