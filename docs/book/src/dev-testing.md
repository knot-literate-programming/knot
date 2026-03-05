# Testing

## Running the tests

```bash
# All unit tests (no R/Python required)
cargo test --workspace --exclude knot-cli

# Single crate
cargo test -p knot-core

# Include integration tests (requires R and Python installed)
cargo test --workspace
```

`knot-cli` is excluded by default because its integration tests run full
compilations and require live R and Python installations.

---

## Snapshot tests

`knot-core`'s backend tests use [insta](https://insta.rs) for snapshot testing.
All snapshots live in `crates/knot-core/src/snapshots/` and
`crates/knot-core/src/compiler/snapshots/`.

### Running and updating

```bash
# Run snapshot tests normally
cargo test -p knot-core

# Regenerate all snapshots after intentional changes
INSTA_UPDATE=always cargo test -p knot-core

# Review pending snapshot changes interactively
cargo insta review
```

When you add a new test using `assert_snapshot!`:

1. Write the test with the `assert_snapshot!` call.
2. Run `INSTA_UPDATE=always cargo test -p knot-core` once.
3. Verify the generated `.snap` file looks correct.
4. Commit both the test and the `.snap` file.

### What to snapshot test

- Any function in `backend.rs` that produces `.typ` text.
- Any assembler output in `compiler/mod.rs`.
- Parser output for representative inputs.

---

## Integration tests

Integration tests in `knot-cli/tests/` compile actual `.knot` documents and
verify the output. They are marked `#[ignore]` so they do not run in CI (which
does not have R or Python). Run them manually:

```bash
cargo test -p knot-cli -- --ignored
```

---

## Test organisation conventions

- Test functions are named `<thing>_should_<expectation_when_condition>`, for
  example `format_chunk_should_omit_stroke_when_no_border_option`.
- Each test has one assertion (or one `assert_snapshot!` call).
- Tests that require external tools are `#[ignore]` with a comment explaining
  the dependency.

---

## Current coverage gaps

The most critical gap (tracked as Technical Debt A in the master plan) is that
the R and Python executor code paths are covered only by `#[ignore]` tests.
The execution pipeline itself — `pipeline.rs`, `execution.rs`, `freeze.rs` —
is exercised only by integration tests.

If you add new execution logic, consider whether it can be tested with a mock
executor that returns fixed `ExecutionAttempt` values without spawning a
real interpreter.
