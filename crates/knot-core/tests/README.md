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

- **Unit tests:** 13 tests (parser, cache, graphics)
- **Integration tests (basic):** 5 tests
- **Integration tests (execution):** 7 tests (require R, ignored by default)

**Total:** 18 tests + 7 ignored tests = **25 tests**

All tests passing ✅
