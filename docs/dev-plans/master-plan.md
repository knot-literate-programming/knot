# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project. Detailed designs for specific features are located in their respective files within this directory.

## 🏗️ Project Overview

- **Core**: The engine (parsing, execution, cache).
- **LSP**: Language Server for IDE support (VS Code).
- **CLI**: Command-line interface for batch processing and watch mode.

## 📊 Current Status (Updated Feb 2026)

**Maturity:** ~85% towards v1.0

**Core:** ~98% complete
- ✅ Parsing, execution, caching
- ✅ Multi-language (R, Python)
- ✅ Chunk customization with show: none and aliases
- ✅ Unified structured error handling & Granular resilience
- ✅ Customizable warnings (styles & visibility)

**LSP:** ~75% complete
- ✅ Hover (chunks, R, Python, Typst)
- ✅ Completion (chunk options, R, Python, Typst)
- ✅ Diagnostics (parsing errors, structure validation)
- ✅ Document symbols (including all show variants)
- ⏳ Go to Definition
- ⏳ Hybrid formatting (Air/Ruff)

**CLI:** ~95% complete
- ✅ Compile, watch, init
- ✅ Dynamic knot.toml generation (Unified structures)

**Documentation:** ~45% complete
- ✅ Dev plans (architecture, design docs)
- ⏳ User documentation (getting started, tutorials)
- ⏳ Example projects (reports, dashboards)

---

## 🎯 Current Priorities

### 1. IDE Integration (LSP Diagnostics)
Now that the Core captures structured errors and warnings, they must be surfaced in the editor.
- [x] **Core Capture**: Structured errors/warnings are captured and persisted.
- [ ] **Live Diagnostics**: Feed executor errors back to the LSP for real-time red underlines in code chunks.
- [ ] **Warning Surfacing**: Display R/Python warnings as diagnostic messages in VS Code.

### 2. Standard IDE Navigation (LSP)
Make Knot feel like a native editor for both Typst and the embedded languages.
- [x] **Hover**: Implemented for Knot chunks, R, Python, and Typst (via Tinymist proxy).
- [x] **Completion**: Implemented for chunk options, R, Python, and Typst (via Tinymist proxy).
- [ ] **Go to Definition**: Navigate to function/variable definitions.
- [ ] **References**: Find all references to symbols.

### 3. Documentation & Examples
Make Knot accessible and showcase its capabilities.
- [ ] **User Guide**: Getting started, installation, basic usage
- [ ] **Tutorial**: Step-by-step walkthrough of features
- [ ] **Example Projects**: Scientific report, data dashboard, technical book
- [ ] **API Documentation**: Document all chunk options and their effects

### 4. Advanced Features (Future)
- [ ] **Sync Mapping**: Bidirectional click (Source ↔ PDF). See [sync-mapping.md](sync-mapping.md).
- [ ] **Julia Support**: Extend to Julia language
- [ ] **Content Generators**: Mermaid diagrams, LilyPond music notation

---

## 📅 Roadmap by Component

### Knot Core
- [x] YAML-based options parsing (standardized).
- [x] Macro-based options definition (`define_options!`).
- [x] **Unified Configuration**: Single source of truth for YAML and TOML options.
- [x] **Chunk display customization**: Integrated `show` option and aliases.
- [x] **Robust Execution**: Zero-escape model using temp files for all languages.
- [x] **Graceful Degradation**: Granular per-language resilience.
- [ ] (Future) Support for Julia executor.
- [ ] (Future) Support for Content-Generators (Mermaid, LilyPond).

### Knot LSP
- [x] Bi-directional diagnostics tunnel for Typst.
- [x] Precise line-offset for Knot structure errors.
- [x] **Hover**: For chunks, R/Python code, and Typst (via Tinymist proxy).
- [x] **Completion**: For chunk options, R/Python code, and Typst.
- [x] **Document Symbols**: Structure outline.
- [x] **Position Mapping**: Knot ↔ Typst coordinate translation.
- [ ] **Go to Definition**: Navigate to symbols across languages.
- [ ] **References**: Find all uses of a symbol.
- [ ] **Hybrid Formatting**: Air (R) + Ruff (Python) + Tinymist (Typst).
- [ ] **Variable Explorer**: Dynamic introspection of R/Python sessions.

### Knot CLI
- [x] Watch mode.
- [ ] Improved error logging for CI/CD.
- [x] Built-in project initialization (`knot init`).

---

## 🔗 Design Documents
- [r-error-handling.md](r-error-handling.md): Unified error model (R implementation).
- [python-error-handling.md](python-error-handling.md): Unified error model (Python implementation).
- [sync-mapping.md](sync-mapping.md): PDF-to-Source synchronization plan.
- [lsp-navigation.md](lsp-navigation.md): Roadmap for navigation features.
- [formatters.md](formatters.md): Plan for Air (R) and Ruff (Python) integration.
- [chunk-customization.md](chunk-customization.md): Flexible chunk presentation options.
