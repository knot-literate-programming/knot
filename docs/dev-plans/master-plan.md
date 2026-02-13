# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project. Detailed designs for specific features are located in their respective files within this directory.

## 🏗️ Project Overview

- **Core**: The engine (parsing, execution, cache).
- **LSP**: Language Server for IDE support (VS Code).
- **CLI**: Command-line interface for batch processing and watch mode.

## 📊 Current Status (Updated Feb 2026)

**Maturity:** ~90% towards v1.0

**Core:** ~99% complete
- ✅ Parsing, execution, caching
- ✅ Multi-language (R, Python)
- ✅ Chunk customization with show: none and aliases
- ✅ Unified structured error handling & Granular resilience
- ✅ Customizable warnings (styles & visibility)
- ✅ Zero-escape robust execution model

**LSP:** ~85% complete
- ✅ Hover (chunks, R, Python, Typst — stable & responsive)
- ✅ Completion (chunk options, R, Python, Typst)
- ✅ Diagnostics (parsing errors, structure validation)
- ✅ **Execution Diagnostics**: Runtime errors and warnings surfaced in the editor from cache.
- ✅ Document symbols (including all show variants)
- ⏳ Go to Definition
- ⏳ Hybrid formatting (Air/Ruff)

**CLI:** ~98% complete
- ✅ Compile, watch, init
- ✅ Dynamic knot.toml generation (Unified structures)

**Documentation:** ~50% complete
- ✅ Dev plans (architecture, design docs updated)
- ⏳ User documentation (getting started, tutorials)
- ⏳ Example projects (reports, dashboards)

---

## 🎯 Current Priorities

### 1. IDE Navigation & Polishing (LSP)
Make Knot feel like a native editor for both Typst and the embedded languages.
- [x] **Stable Hover/Completion**: Reliability across all document sections.
- [ ] **Go to Definition**: Navigate to function/variable definitions.
- [ ] **References**: Find all references to symbols.
- [ ] **Unknown Option Warnings**: Validate YAML options to catch typos.

### 2. Standard Tooling Integration
- [ ] **Hybrid Formatting**: Air (R) + Ruff (Python) + Tinymist (Typst).
- [ ] **Julia Support**: Extend the robust execution model to Julia.

### 3. Documentation & Examples
Make Knot accessible and showcase its capabilities.
- [ ] **User Guide**: Getting started, installation, basic usage.
- [ ] **Scientific Proof of Concept**: A complete project demonstrating multi-language constant objects and complex layout.

### 4. Advanced Features (Future)
- [ ] **Sync Mapping**: Bidirectional click (Source ↔ PDF). See [sync-mapping.md](sync-mapping.md).
- [ ] **Variable Explorer**: Dynamic introspection of R/Python sessions.

---

## 📅 Roadmap by Component

### Knot Core
- [x] **Unified Configuration**: Single source of truth for YAML and TOML options.
- [x] **Robust Execution**: Zero-escape model using temp files for all languages.
- [x] **Graceful Degradation**: Granular per-language resilience.
- [ ] (Future) Support for Julia executor.

### Knot LSP
- [x] **Position Mapping**: Robust UTF-16 aware coordinate translation.
- [x] **Runtime Diagnostics**: Errors and warnings from build/watch surfaced in VS Code.
- [ ] **Go to Definition**: Navigate to symbols across languages.
- [ ] **Hybrid Formatting**: Air (R) + Ruff (Python) + Tinymist (Typst).

### Knot CLI
- [x] **Watch mode**: with real-time feedback loop to the editor.
- [x] **Project initialization**: `knot init`.

---

## 🔗 Design Documents
- [lsp-diagnostics.md](lsp-diagnostics.md): Logic for surfacing runtime errors in the editor.
- [r-error-handling.md](r-error-handling.md): Unified error model implementation.
- [python-error-handling.md](python-error-handling.md): Unified error model implementation.
- [sync-mapping.md](sync-mapping.md): PDF-to-Source synchronization plan.
- [lsp-navigation.md](lsp-navigation.md): Roadmap for navigation features.
- [formatters.md](formatters.md): Plan for Air (R) and Ruff (Python) integration.
- [chunk-customization.md](chunk-customization.md): Flexible chunk presentation options.
