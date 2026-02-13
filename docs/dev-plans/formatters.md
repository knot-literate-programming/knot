# Code Formatters Integration (Hybrid Strategy)

**Goal:** Provide professional-grade, unified code formatting for Typst, R, and Python within Knot documents.

## 🛠️ Tool Choices

### R: Air
- **Reason**: Written in Rust, extremely fast, official successor to `styler`.
- **Status**: Skeleton `AirFormatter` already exists in `knot-lsp`.

### Python: Ruff
- **Reason**: The industry standard for high-performance Python linting and formatting, written in Rust.
- **Goal**: Use `ruff format` via stdin/stdout for instant feedback.

### Typst: Tinymist (Proxy)
- **Reason**: Best-in-class Typst formatting, already integrated into our LSP proxy.

## 📋 Implementation Strategy: The "Funnel" Approach

To avoid the **"Offset Conflict"** (where Typst formatting moves a chunk, making the Python/R offsets invalid), we will implement a sequential formatting pipeline:

### 1. Step A: Internal Chunk Formatting
- **Action**: Parse the `.knot` file and extract the raw code of each R and Python chunk.
- **Process**: Send the code to `air` or `ruff`.
- **Constraint**: The formatter must be configured to ignore the surrounding Typst context and return only the formatted code block.
- **Indentation**: We must detect the base indentation of the chunk's opening fence (e.g., if it's inside a list) and ensure the formatter respects or preserves this relative indentation.

### 2. Step B: Document Reconstruction
- **Action**: Replace the messy code in the original Knot document with the formatted code from Step A.
- **Result**: We now have a Knot document where every code block is clean, but the Typst spacing/alignment might still be messy.

### 3. Step C: Global Typst Formatting
- **Action**: Transform the "partially cleaned" Knot document into a virtual Typst document (standard mask strategy).
- **Process**: Send the result to `tinymist` via `textDocument/formatting`.
- **Result**: Tinymist cleans up the Typst syntax, headings, and spacing around the chunks.

### 4. Final Step: Delta Generation
- **Action**: Compare the final result with the original document to generate a list of `TextEdit` objects for the LSP.

## ⚠️ Challenges & Technical Notes

- **Indentation Leaks**: Python is extremely sensitive to indentation. If Typst formatting changes the indentation of a line containing a chunk, it might break the Python code. We must ensure that Step C treats the masked chunk areas as atomic blocks that should not have their internal relative indentation modified.
- **Formatting on Save**: This pipeline must be fast. Using Rust-based tools (`Air`, `Ruff`, `Tinymist`) is crucial here to keep the "Save & Format" cycle under 200ms.
- **Configuration**:
    - Binaries paths (`ruff`, `air`) must be configurable in VS Code settings.
    - Users should be able to enable/disable formatting for specific languages.

## 🎯 Success Criteria
- [ ] Messy R code (`x<-1+1`) is cleaned to `x <- 1 + 1` on save.
- [ ] Python docstrings and indentation are standardized.
- [ ] Typst headings and whitespace are normalized.
- [ ] Formatting is idempotent (running it twice produces no changes).
- [ ] **Zero Overlap**: Formatting a chunk never corrupts the surrounding Typst syntax.
