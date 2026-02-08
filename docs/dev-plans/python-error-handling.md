# Python Error Handling: Parity with R

**Goal:** Provide structured error detection for Python chunks, matching the level of detail planned for R.

## 📋 Implementation Plan

### 1. Structured Traceback Capture
Modify the Python executor to wrap user code in a global `try...except` block that captures the full traceback and formats it into a recognizable structure.

```python
try:
    # User code here
    pass
except Exception as e:
    import traceback
    print("__KNOT_ERROR_START__", file=sys.stderr)
    print(f"MESSAGE: {e}", file=sys.stderr)
    print("TRACEBACK:", file=sys.stderr)
    traceback.print_exc(file=sys.stderr)
    print("__KNOT_ERROR_END__", file=sys.stderr)
```

### 2. Rust Parsing
Implement a `parse_structured_error` method in `crates/knot-core/src/executors/python/execution.rs` similar to the R implementation.

### 3. Reporting
- Extract the line number from the Python Traceback.
- Convert the "chunk-relative line" to a "document-absolute line".
- Return a structured error that the LSP can consume.

## 🎯 Success Criteria
- [ ] Unknown variables in Python are underlined in red at the correct line.
- [ ] Logic errors (e.g., ZeroDivisionError) report the exact line inside the chunk.
- [ ] Error messages include the Python exception type.
