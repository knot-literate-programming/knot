# LSP Diagnostics — Runtime Warnings & Errors

**Status:** ✅ Implemented
**Date:** 2026-02-13

## Goal

Surface R/Python runtime warnings and errors as LSP diagnostics (squiggles) in the editor,
integrated into the workflow after execution (build or watch).

## Implementation (2026-02-13)

### 1. Unified Diagnostic Flow
Knot now combines three sources of diagnostics:
- **Structure**: Parsing errors and invalid syntax (captured by `Document::parse`).
- **Options**: Validation of YAML chunk options against known fields.
- **Runtime**: Warnings and Errors persisted in the `.knot_cache/metadata.json` after execution.

### 2. Precise Positioning
While initially thought to be chunk-level only, we achieved **line-level precision**:
- **Python**: Captures `lineno` for warnings and parses tracebacks for errors to find the exact line relative to the chunk start.
- **R**: Highlights the specific line for errors when available in the message, and falls back to highlighting the closing triple backticks (```) for warnings to minimize visual noise.
- **UTF-16 Awareness**: The `PositionMapper` ensures that coordinates are correctly translated between Rust (UTF-8 bytes) and LSP (UTF-16 code units).

### 3. Graceful Degradation & Feedback
- If a fatal error occurs, the language is marked as "broken".
- Subsequent chunks of the same language are rendered as **inert** (grayed out) in the PDF and editor.
- The editor surfaces the full traceback/call info on hover, while the PDF keeps a concise one-line summary for professional rendering.

## ⚠️ Known Issues / Future Work

### 1. Unknown chunk options validation
Currently, unknown options in a chunk are silently ignored. We should implement a validation step using `ChunkOptions::option_metadata()` to warn the user about typos (e.g. `warinings-visibility`).

### 2. Live syntax diagnostics (no execution)
The background executors could validate syntax in real-time on `did_change` without touching the actual environment, providing immediate feedback before the user saves or builds.

---

## 1:1 Position Mapping Invariant
The `.knot` ↔ `.typ` position mapping remains identity-based (padding with spaces/newlines). This invariant must be preserved for any change to virtual document generation to keep the current mapping robustness.
