# Python Error Handling: Parity with R

**Goal:** Provide structured error detection for Python chunks, matching the level of detail implemented for R.

## 📋 Implementation Plan (Implemented 2026-02-13)

### 1. Structured Traceback Capture
Modify the Python executor to wrap user code in a global `try...except` block that captures the full traceback and writes it to the structured side-channel metadata.

```python
with _knot_wm.catch_warnings(record=True) as _knot_caught:
    try:
        with open('{code_file}', 'r', encoding='utf-8') as _knot_f:
            _knot_c = compile(_knot_f.read(), '{code_file}', 'exec')
            exec(_knot_c, globals())
    except Exception as _knot_e:
        _write_metadata({'message': str(_knot_e), 'traceback': _knot_tb.format_tb(_knot_e.__traceback__)}, type='error')
        raise
```

### 2. Rust Integration
- Integrated into the unified `process_execution_output` in `crates/knot-core/src/executors/mod.rs`.
- Standardized the use of temporary files (`knot_code_*.py`) for zero-escape robustness.

### 3. Graceful Degradation
- Implemented granular resilience: if Python fails, subsequent Python chunks are marked as **inert** and rendered with a visual "gray out" effect.
- R chunks continue to execute normally.

## 🎯 Success Criteria
- [x] Python logic errors (e.g., ZeroDivisionError) are captured with structured metadata.
- [x] Error messages include the Python exception type and traceback.
- [x] Syntax errors are caught by the wrapper thanks to `compile()`.
- [x] Parity with R is achieved through a unified side-channel flow.

---

**Status:** ✅ Implemented
**Last Updated:** 2026-02-13

## ✅ Implementation notes (2026-02-13)

- The implementation achieved total parity with R by using temporary files for user code execution.
- Python traceback is cleaned by skipping 1 internal frame (the `exec()` call).
- Configuration layering (Global < Language < Error < Chunk) allows custom styling for skipped chunks via the `[python-error]` section in `knot.toml`.
