# Testing

## Rust checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

The default suite includes `knot-cli`. Its project assembly tests do not
require Typst, R or Python. Tests requiring external interpreters or Typst
are explicitly ignored in the default run and executed separately below.

## R and Python integration tests

Install R with `ggplot2`, `svglite`, `jsonlite` and `digest`, and Python with
`pandas` and `matplotlib`. The executors launch `R` and `python3` from PATH;
activate your Python virtual environment before running the tests.

For headless graphics on macOS/Linux:

```bash
export MPLBACKEND=Agg
export MPLCONFIGDIR="${TMPDIR:-/tmp}/knot-matplotlib"
cargo test -p knot-core --locked -- --ignored
```

On PowerShell, set `$env:MPLBACKEND = 'Agg'` and
`$env:MPLCONFIGDIR = "$env:TEMP/knot-matplotlib"` before the same Cargo command.
The writable Matplotlib directory avoids relying on a user-level font cache.

These tests cover interpreter execution, errors, timeouts, figures, tables,
inline expressions, mixed-language documents and session snapshots.

## Cache regression tests

`cache_correctness.rs` exercises document isolation, dependency roots, planning,
artifact copying, corrupted files, cold/warm/clean builds, Python snapshot replay,
and R/Python snapshot policy and Python shared-reference preservation. The default suite also uses a fake executor to
check replay, cancellation and inert cascades without installing interpreters.

```bash
cargo test -p knot-core --locked --test cache_correctness
# With the R/Python requirements above:
cargo test -p knot-core --locked --test cache_correctness -- --include-ignored
```

## Publication and concurrency regressions

`project_publication.rs` checks coherent buffer capture, private rendering,
cancelled publication, and published Python/R artifacts and snapshots surviving
workspace cleanup. The runtime cases run with the ignored core suite above.
The fake executor test pauses inside a chunk, cancels it, and verifies that neither
a result nor a snapshot is cached and the following chunk never executes.

LSP tests use barriers/channels rather than sleeps to force publication ordering.
They cover typing invalidation, independent projects, main/include buffer capture,
failed preparation/publication, and an old worker finishing after the new result
has been published. The last case checks real Typst/cache files and completion
notifications. VS Code's `npm test` checks late events, include edits, failures,
local version changes and independent project generations without launching VS Code.

## CLI and PDF integration tests

```bash
# Project assembly, including missing/out-of-root includes: no Typst needed
cargo test -p knot-cli --locked --test integration_build

# Actual knot build subprocess, PDF output and invalid Typst: Typst required
cargo test -p knot-cli --locked --test integration_pdf -- --ignored
```

PDF tests use plain Typst documents and require neither R/Python nor downloads
of Typst packages. CI installs Typst 0.15.1 explicitly for this step.

## Tinymist integration tests

CI installs the official **Tinymist 0.15.8** binary, verifies its published
SHA-256 checksum, and runs the LSP integration suite on Linux, macOS and Windows.
No R/Python, Typst package downloads or editor installation are needed for these tests.

To reproduce locally, install that version and place it first in `PATH`:

```bash
python3 scripts/ci/install-tinymist.py 0.15.8 /tmp/knot-tinymist
export PATH="/tmp/knot-tinymist:$PATH"
tinymist --version
cargo test -p knot-lsp --locked -- --ignored
```

On Windows, use `python scripts/ci/install-tinymist.py 0.15.8 "$env:TEMP/knot-tinymist"`
and prepend that directory to `$env:PATH` before the same Cargo command.
The installer supports x86-64 and ARM64 release archives.

These tests are ignored only in the default suite. Explicit execution fails if
Tinymist is missing or cannot initialize. They verify initialization and shutdown,
then open a temporary document with an undefined symbol, check the diagnostic,
correct the document through `didChange`, and check that diagnostics are cleared.
The exchange has a 30-second deadline and shuts down the server before reporting
an assertion failure. A default-suite test uses a portable fake server to verify
response/write deadlines, pending-request cleanup and forced process termination.

## VS Code extension

From `editors/vscode`:

```bash
npm ci
npm run typecheck
npm run lint
npm test
npm run compile
npm exec -- vsce package --out test.vsix
```

Bundling with esbuild does not check types. `typecheck` runs `tsc --noEmit`,
and the packaging prepublish hook also checks types before producing a VSIX.
The packager comes from the project's locked dependencies.

## Snapshot tests

Backend and assembler snapshots use `insta` and live under
`crates/knot-core/src/snapshots/` and
`crates/knot-core/src/compiler/snapshots/`.

```bash
cargo test -p knot-core --locked
# Only after an intentional output change:
INSTA_UPDATE=always cargo test -p knot-core --locked
cargo insta review
```

Review changed snapshots before committing them; do not regenerate snapshots
just to hide a test failure.

## CI and remaining limits

CI runs workspace tests, R/Python integration tests and CLI PDF tests on Linux,
macOS and Windows. It records runtime and dependency versions. Rust formatting
and Clippy run on Linux, as do extension type checks, lint, bundling and packaging.

For each PR, a new commit cancels the previous run of this CI workflow. Runs on
`master` and release tags remain independent. The three-platform matrix and all
test commands are unchanged by this optimization.

Dependency caches are managed by the existing setup actions:

- Rust uses `Swatinem/rust-cache`; restoration happens before R setup so Rtools
  does not affect archive-tool selection on Windows.
- R uses `r-lib/actions/setup-r-dependencies`, with an explicit package list and
  required dependencies only. It respects the repositories configured by
  `setup-r`, including binary packages when available. Its cache distinguishes
  OS, R version, architecture and resolved dependencies; increment the action's
  `cache-version` input if it needs invalidating.
- pip caches downloads using `examples/anscombe/requirements.txt` as its key input.
- npm caches downloads using `editors/vscode/package-lock.json` as its key input.

Package installation and tests still run on cache hits. Caches do not replace
version locks or guarantee unchanged upstream dependencies. Compare successful
runs with warm caches when assessing time savings; the first run fills them.

Two ignored `knot-lsp` tests require Tinymist and are not part of this CI suite.
Neither the default tests nor VSIX packaging validate an interactive VS Code
preview session. Navigation and unversioned upstream Tinymist diagnostics still
need dedicated coverage. Interpreter versions and R/Python package versions are recorded, but
are not all pinned; consult the CI logs when investigating a regression.
