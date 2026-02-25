# Document Backends & Format Abstraction

This document outlines the strategy for decoupling code execution from document-specific formatting. The goal is to move away from hardcoded Typst strings within executors and move towards a pluggable backend architecture.

## 🎯 Objectives

- **Decoupling**: Separate "what to show" (data/plots) from "how to show it" (Typst/LaTeX syntax).
- **Maintainability**: Centralize formatting logic to simplify updates and ensure consistency.
- **Extensibility**: Pave the way for LaTeX support without duplicating execution logic.
- **Native Quality**: Avoid generic intermediate formats (like Pandoc's AST) to ensure high-quality, idiomatic output for each target language.

## 🏗️ Architectural Vision

### 1. The `DocumentBackend` Trait

A core trait in `knot-core` that defines the high-level formatting interface:

```rust
pub trait DocumentBackend {
    // Structural elements
    fn table(&self, data: &DataFrame) -> String;
    fn image(&self, path: &Path, caption: Option<&str>, width: Option<&str>) -> String;
    fn inline_code(&self, code: &str) -> String;
    
    // Layout & Styling
    fn grid(&self, elements: Vec<String>, columns: usize) -> String;
    fn alert(&self, message: &str, level: AlertLevel) -> String;
}
```

### 2. Backend Implementations

- **`TypstBackend`**: Generates idiomatic Typst code (e.g., `#table`, `#image`, `#grid`).
- **`LatexBackend` (Future)**: Generates idiomatic LaTeX code (e.g., `\begin{tabular}`, `\includegraphics`).

### 3. Language-Side Support (Resources)

Formatters in R and Python should eventually leverage language-specific "backend scripts" stored in `knot-core/resources/`:

- `resources/python/backends/typst.py`
- `resources/r/backends/typst.R`

## 🚀 Implementation Phases

### Phase 1: Core Abstraction (Typst-First)
- [ ] Define the `DocumentBackend` trait in `knot-core/src/backend.rs`.
- [ ] Implement `TypstBackend`.
- [ ] Refactor `knot-core/src/executors/python/formatters.rs` and `knot-core/src/executors/r/formatters.rs` to use the trait.
- [ ] Pass the active backend through the `ExecutionManager`.

### Phase 2: Template Externalization
- [ ] Move hardcoded templates from `knot-core/src/defaults.rs` to a `templates/` directory.
- [ ] Use `include_str!` or a runtime loader to fetch templates based on the target backend.

### Phase 3: LaTeX Foundation
- [ ] Add a `target_format` option to `knot.toml`.
- [ ] Implement a basic `LatexBackend`.
- [ ] Implement "Valid Source" logic for LaTeX (supporting `{r}` and `{python}` blocks within `lstlisting` or `verbatim`).

## 📋 Design Principles

1. **Source Validity**: A `.knot` file should always be a valid document in its target language (Typst or LaTeX).
2. **Zero-Overhead Abstraction**: The trait should not introduce significant performance penalties.
3. **Idiomatic Output**: The generated code must look like it was written by a human expert in that language, avoiding the "robotic" look of generic converters.

## 🔗 Related Documents
- [master-plan.md](master-plan.md): High-level project roadmap.
- [formatters.md](formatters.md): Current implementation of R and Python formatters.
