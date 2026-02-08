# Code Formatters Integration

**Goal:** Provide professional-grade code formatting for R and Python chunks within Knot documents.

## 🛠️ Tool Choices

### R: Air
- **Reason**: Written in Rust, extremely fast, official successor to `styler`.
- **Status**: Skeleton `AirFormatter` already exists in `knot-lsp`.

### Python: Ruff
- **Reason**: Industry standard for fast Python linting and formatting, written in Rust.
- **Goal**: Use `ruff format` command.

## 📋 Implementation Plan

### 1. Hybrid Formatting Strategy
Since a `.knot` file contains Typst, R, and Python, we cannot use a single formatter.
1. **LSP Action**: User triggers "Format Document".
2. **Step A (Typst)**: Delegate full document formatting to Tinymist (via virtual URI).
3. **Step B (Chunks)**: 
   - Parse the document to find R/Python chunks.
   - For each chunk, run the respective formatter (`air` or `ruff`).
4. **Step C (Merge)**: Combine `TextEdit` objects from all steps, ensuring no overlaps or conflicts.

### 2. Configuration
- Add settings in VS Code to specify paths to `air` and `ruff` binaries.
- Option to "Format on Save".

## 🎯 Success Criteria
- [ ] Messy R code inside a chunk is cleaned up on save.
- [ ] Python indentation and style are standardized.
- [ ] Formatting does not break chunk boundaries or Typst syntax.
