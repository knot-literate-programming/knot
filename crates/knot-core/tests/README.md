# Integration Tests

This directory contains integration tests for knot-core.

## Test Files

### `integration_basic.rs`
Tests for basic document parsing functionality:
- Simple chunk parsing
- Multiple chunks
- Chunks with and without names
- Empty documents
- Dependency tracking

**Run with:** `cargo test --test integration_basic`

### `integration_execution.rs`
Tests for R code execution and output handling:
- Simple R execution
- Error handling
- R session persistence
- DataFrame serialization (requires `knot.r.package`)
- Plot generation (requires `ggplot2` and `knot.r.package`)
- Combined DataFrame + Plot output
- Warning vs error distinction

**Run with:** `cargo test --test integration_execution -- --ignored`

**Note:** These tests require R and are ignored by default. Use `--ignored` to run them.

### `integration_inline.rs`
Tests for inline expression parsing and execution:
- Inline expression detection and parsing
- Escaped expressions (`\#r[x]`)
- Multiple expressions in same document
- Scalar execution (`#r[x]` → `42`)
- String execution (`#r[name]` → `Alice`)
- Vector execution (`#r[1:5]` → `` `[1] 1 2 3 4 5` ``)
- Arithmetic and function calls
- Integration with chunk variables
- Nested bracket handling (`#r[letters[1:3]]`)
- Error handling (complex outputs rejected)

**Run with:**
- `cargo test --test integration_inline` (parsing tests only, no R)
- `cargo test --test integration_inline -- --ignored` (all tests, requires R)

## Running Tests

### All tests (unit + integration)
```bash
cargo test
```

### Only integration tests
```bash
cargo test --test integration_basic
cargo test --test integration_execution -- --ignored
```

### Specific test
```bash
cargo test test_simple_r_execution -- --ignored
```

## Test Results Summary

- **Unit tests:** 21 tests (parser, cache, graphics, inline expressions)
- **Integration tests (basic):** 5 tests
- **Integration tests (execution):** 7 tests (require R, ignored by default)
- **Integration tests (inline):** 11 tests (2 parsing + 9 execution with R)

**Total:** 28 tests (by default) + 16 ignored tests (with R) = **44 tests**

All tests passing ✅
