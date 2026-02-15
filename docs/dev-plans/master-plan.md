# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project. Detailed designs for specific features are located in their respective files within this directory.

## 🏗️ Project Overview

- **Core**: The engine (parsing, execution, cache).
- **LSP**: Language Server for IDE support (VS Code).
- **CLI**: Command-line interface for batch processing and watch mode.

## 📊 Current Status (Updated Feb 2026)

**Maturity:** ~92% towards v1.0

**Core:** ~99% complete
- ✅ Parsing, execution, caching
- ✅ Multi-language (R, Python)
- ✅ Chunk customization with show: none and aliases
- ✅ Unified structured error handling & Granular resilience
- ✅ Customizable warnings (styles & visibility)
- ✅ Zero-escape robust execution model (temp files)
- ✅ **Centralized Structural Formatting**: Unified logic for CLI and LSP in `Document::format`.

**LSP:** ~97% complete
- ✅ Hover (chunks, R, Python, Typst — stable & responsive)
- ✅ **Dynamic Completion**: Chunk options suggested based on core metadata (with docs & defaults).
- ✅ Diagnostics (parsing errors, structure validation)
- ✅ **Unknown Option Warnings**: Validated against `OptionMetadata` to catch typos.
- ✅ **Execution Diagnostics**: Runtime errors and warnings surfaced in the editor from cache.
- ✅ Document symbols (including all show variants)
- ✅ **Architectural Hardening**: Consolidated state, lock contention minimized, and proper virtual URI versioning.
- ✅ **Hybrid formatting**: Full 3-phase pipeline (Air → Tinymist → reconstruction) with async pre-calculation and graceful fallbacks.
- ⏳ Go to Definition & References
- ⏳ Error surfacing for Tinymist failures (`window/showMessage`)

**CLI:** ~99% complete
- ✅ Compile, watch, init
- ✅ Dynamic knot.toml generation (Unified structures)
- ✅ **Thread-safe Integration Tests**: Validated for parallel execution.
- ✅ **Centralized Formatting**: `knot format` uses the same engine as the LSP.

---

## 🎯 Current Priorities

### 1. IDE Navigation & Polishing (LSP)
Make Knot feel like a native editor for both Typst and the embedded languages.
- [x] **Stable Hover/Completion**: Reliability across all document sections.
- [ ] **Go to Definition**: Navigate to function/variable definitions.
- [ ] **References**: Find all uses of a symbol across the document.
- [ ] **Unknown Option Warnings**: Validate YAML options against `OptionMetadata` to catch typos.

### 2. Standard Tooling Integration
- [x] **Hybrid Formatting**: Full 3-phase pipeline implemented in LSP (Air + Ruff + Tinymist). See [formatters.md](formatters.md).
- [ ] **Formatter error surfacing**: Forward Tinymist `window/showMessage` to client; generate notifications for Air/Ruff failures (at most once per session).
- [ ] **Julia Support**: Extend the robust execution model to the Julia language.

### 3. Documentation & Examples
Make Knot accessible and showcase its capabilities.
- [ ] **User Guide**: Getting started, installation, basic usage.
- [ ] **Tutorial**: Step-by-step walkthrough of features.
- [ ] **Scientific Proof of Concept**: A complete project demonstrating multi-language constant objects and complex layout.
- [ ] **API Documentation**: Automated documentation of all chunk options and their effects.

### 4. Advanced Features (Future)
- [ ] **Sync Mapping**: Bidirectional click (Source ↔ PDF). See [sync-mapping.md](sync-mapping.md).
- [ ] **Variable Explorer**: Dynamic introspection of R/Python sessions in the editor.
- [ ] **Content Generators**: Support for Mermaid diagrams, LilyPond music notation, etc.

---

## 📅 Roadmap by Component

### Knot Core
- [x] **Unified Configuration**: Single source of truth for YAML and TOML options.
- [x] **Robust Execution**: Zero-escape model using temp files for all languages.
- [x] **Graceful Degradation**: Granular per-language resilience.
- [ ] (Future) Support for Julia executor.
- [ ] (Future) Support for Content-Generators (Mermaid, LilyPond).

### Knot LSP
- [x] **Position Mapping**: Robust UTF-16 aware coordinate translation.
- [x] **Runtime Diagnostics**: Errors and warnings from build/watch surfaced in VS Code.
- [x] **Hybrid Formatting**: Full 3-phase pipeline (Air + Ruff + Tinymist) with mirror mask and reconstruction guards.
- [ ] **Go to Definition**: Navigate to symbols across languages.
- [ ] **Formatter error surfacing**: Notify user on Air/Ruff/Tinymist failures via `window/showMessage`.
- [ ] **Variable Explorer**: Interactive introspection of live sessions.

### Knot CLI
- [x] **Watch mode**: with real-time feedback loop to the editor.
- [x] **Project initialization**: `knot init`.
- [ ] Improved error logging for CI/CD environments.

---

## 🔗 Design Documents
- [lsp-diagnostics.md](lsp-diagnostics.md): Logic for surfacing runtime errors in the editor.
- [r-error-handling.md](r-error-handling.md): Unified error model implementation.
- [python-error-handling.md](python-error-handling.md): Unified error model implementation.
- [sync-mapping.md](sync-mapping.md): PDF-to-Source synchronization plan.
- [lsp-navigation.md](lsp-navigation.md): Roadmap for navigation features.
- [formatters.md](formatters.md): Plan for Air (R) and Ruff (Python) integration.
- [chunk-customization.md](chunk-customization.md): Flexible chunk presentation options.
