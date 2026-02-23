# Knot Project Master Plan

This document tracks the high-level goals and roadmap for the Knot project. Detailed designs for specific features are located in their respective files within this directory.

## 🏗️ Project Overview

- **Core**: The engine (parsing, execution, cache).
- **LSP**: Language Server for IDE support (VS Code).
- **CLI**: Command-line interface for batch processing and watch mode.

## 📊 Current Status (Updated Feb 2026)

**Maturity:** ~98% towards v1.0

**Core:** ~95% complete
- ✅ Parsing, execution, caching
- ✅ Multi-language (R, Python)
- ✅ Chunk customization with show: none and aliases
- ✅ Unified structured error handling & Granular resilience
- ✅ Customizable warnings (styles & visibility)
- ✅ Zero-escape robust execution model (temp files)
- ✅ **Centralized Structural Formatting**: Unified logic for CLI and LSP in `Document::format`.
- ⏳ **Progressive Compilation**: Implementing the async three-pass pipeline (Plan/Execute/Assemble) for background execution.
- ✅ **Codebase Hardening** (Feb 2026): All critical bugs (C1–C3), data loss risks (D1–D3), design issues (De1–De5), and code quality issues (Q1–Q9) resolved.

**LSP:** ~97% complete
- ✅ Hover (chunks, R, Python, Typst — stable & responsive)
- ✅ **Dynamic Completion**: Chunk options suggested based on core metadata.
- ✅ Diagnostics (parsing errors, structure validation)
- ✅ **Unknown Option Warnings**: Validated against `OptionMetadata`.
- ✅ **Execution Diagnostics**: Runtime errors surfaced in the editor from cache.
- ✅ Document symbols (including all show variants)
- ✅ **Architectural Hardening**: Consolidated state and lock ordering documented.
- ✅ **Hybrid formatting**: Full 3-phase pipeline (Air + Ruff + Tinymist).
- ✅ **Formatter error surfacing**: Tinymist errors forwarded via `window/showMessage`.
- ✅ **Sync Mapping**: Bidirectional click (Source ↔ PDF) with injection markers.
- ⏳ **Live Forward Sync**: Real-time cursor-to-PDF synchronization.
- ⏳ Go to Definition & References

**CLI:** ~99% complete
- ✅ Compile, watch, init
- ✅ Dynamic knot.toml generation
- ✅ **Thread-safe Integration Tests**
- ✅ **Centralized Formatting**

---

## 🎯 Current Priorities

### 1. High-Performance Compilation (Core & LSP)
- [ ] **Async Parallel Pipeline**: Orchestrate the 3-pass model to run executors in the background. See [async-parallel-pipeline.md](async-parallel-pipeline.md).
- [ ] **Live Forward Sync**: Implement real-time synchronization between the editor cursor and the PDF preview.

### 2. IDE Navigation & Polishing (LSP)
Make Knot feel like a native editor for both Typst and the embedded languages.
- [x] **Stable Hover/Completion**: Reliability across all document sections.
- [ ] **Go to Definition**: Navigate to function/variable definitions.
- [ ] **References**: Find all uses of a symbol across the document.
- [x] **Sync Mapping**: Manual bidirectional Source ↔ PDF synchronization.

### 2. Standard Tooling Integration
- [x] **Hybrid Formatting**: Full 3-phase pipeline implemented in LSP (Air + Ruff + Tinymist).
- [x] **Formatter error surfacing**: Forward Tinymist `window/showMessage` to client.
- [ ] **Julia Support**: Extend the robust execution model to the Julia language.

### 3. Documentation & Examples
Make Knot accessible and showcase its capabilities.
- [ ] **User Guide**: Getting started, installation, basic usage.
- [ ] **Tutorial**: Step-by-step walkthrough of features.
- [ ] **Scientific Proof of Concept**: A complete project demonstrating multi-language constant objects and complex layout.
- [ ] **API Documentation**: Automated documentation of all chunk options and their effects.

### 4. Advanced Features (Future)
- [x] **Progressive Compilation**: Already implemented in the core pipeline.
- [ ] **Variable Explorer**: Dynamic introspection of R/Python sessions in the editor.
- [ ] **Content Generators**: Support for Mermaid diagrams, LilyPond music notation, etc.

---

## 📅 Roadmap by Component

### Knot Core
- [x] **Unified Configuration**: Single source of truth for YAML and TOML options.
- [x] **Robust Execution**: Zero-escape model using temp files.
- [x] **Progressive Pipeline**: Three-pass model for background execution.
- [ ] (Future) Support for Julia executor.

### Knot LSP
- [x] **Position Mapping**: Robust UTF-16 aware coordinate translation.
- [x] **Runtime Diagnostics**: Errors and warnings surfaced in VS Code.
- [x] **Hybrid Formatting**: Air + Ruff + Tinymist integration.
- [x] **Sync Mapping**: High-precision PDF-to-Source navigation.
- [x] **Formatter error surfacing**: Notify user on Tinymist failures via `window/showMessage`.
- [ ] **Go to Definition**: Navigate to symbols across languages.
- [ ] **Variable Explorer**: Interactive introspection of live sessions.

### Knot CLI
- [x] **Watch mode**: with real-time feedback loop to the editor.
- [x] **Project initialization**: `knot init`.
- [ ] Improved error logging for CI/CD environments.

---

## 🔗 Design Documents
- [async-parallel-pipeline.md](async-parallel-pipeline.md): Detailed orchestration for background execution.
- [progressive-compilation.md](progressive-compilation.md): Initial strategy for fast, reactive updates.
- [codebase-hardening-2026-02.md](codebase-hardening-2026-02.md): All C1–C3, D1–D3, De1–De5, Q1–Q9 fixes and dependency updates (Feb 2026).
- [lsp-diagnostics.md](lsp-diagnostics.md): Logic for surfacing runtime errors in the editor.
- [r-error-handling.md](r-error-handling.md): Unified error model implementation.
- [python-error-handling.md](python-error-handling.md): Unified error model implementation.
- [sync-mapping.md](sync-mapping.md): PDF-to-Source synchronization plan.
- [lsp-navigation.md](lsp-navigation.md): Roadmap for navigation features.
- [formatters.md](formatters.md): Plan for Air (R) and Ruff (Python) integration.
- [chunk-customization.md](chunk-customization.md): Flexible chunk presentation options.
