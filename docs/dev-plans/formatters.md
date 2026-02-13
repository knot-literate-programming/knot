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

## 📋 Technical Details: Structural Normalization

Beyond external tools, Knot performs "Structural Normalization" to ensure consistency:

- **Header Cleaning**: Extra spaces in ` ```{lang name} ` are stripped.
- **Options Firewall**: Unknown options (except `codly-*`) trigger diagnostics and are preserved but highlighted.
- **YAML Formatting**: Options are strictly formatted as `#| key: value`.
- **Spacing**: A blank line is automatically maintained between the last option and the first line of code if needed.

## ⚠️ Ongoing Challenges

- **Indentation Preservation**: Ensuring that Python chunks inside deeply nested Typst structures (like lists or blocks) maintain correct relative indentation during global Typst formatting.
- **Configuration**: Exposing `air` and `ruff` binary paths through `knot.toml` or VS Code settings (partially implemented).

## 🎯 Success Criteria (Met)
- [x] Messy R code (`x<-1+1`) is cleaned to `x <- 1 + 1`.
- [x] Python docstrings and indentation are standardized via Ruff.
- [x] Typst headings and whitespace are normalized via Tinymist.
- [x] Formatting is idempotent.
- [x] **Performance**: Formatting remains sub-200ms for typical documents.
