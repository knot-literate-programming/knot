# Validation baseline — 2026-09-21

Scope: stage 1 of the stabilization plan, based on commit `b06c15b`.
Branch: `codex/validation-baseline`.

## Local results

Validated on macOS arm64. Commands are documented in
[the testing guide](../book/src/dev-testing.md).

| Check | Result |
|---|---|
| `cargo test --workspace --locked --no-fail-fast` | 109 passed, 46 ignored |
| `cargo test -p knot-core --locked --no-fail-fast -- --ignored` | 42 passed |
| `cargo test -p knot-cli --locked --test integration_pdf -- --ignored` | 2 passed |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --all-targets --all-features --locked -- -D warnings` | Passed |
| Extension typecheck, lint and compile | Passed |
| VSIX packaging with the local locked packager | Passed |

The 46 ignored tests in the default suite comprise the 42 interpreter tests,
2 PDF tests and 2 Tinymist tests. The first two groups were run separately;
the Tinymist tests and interactive editor behavior were not validated.
GitHub Actions has not yet run this branch: Linux and Windows results remain
unconfirmed. No release was published.

## Environment

| Tool/package | Local version |
|---|---|
| Rust / Cargo / Clippy | 1.98.0 / 1.98.0 / 0.1.98 |
| Typst | 0.15.1 |
| R | 4.5.0 |
| ggplot2 / svglite / jsonlite / digest | 4.0.0 / 2.2.2 / 2.0.0 / 0.6.37 |
| Python | 3.14.7 |
| pandas / matplotlib / numpy | 3.0.6 / 3.11.2 / 2.5.3 |
| Node / npm | 26.8.1 / 11.19.0 |
| TypeScript / ESLint / esbuild / vsce | 5.9.3 / 9.39.3 / 0.27.3 / 3.7.1 |

Python dependencies were installed in `/tmp/knot-validation-venv`, without
changing the global Python installation. Integration tests used this environment
on PATH, `MPLBACKEND=Agg` and a writable `MPLCONFIGDIR` under `/tmp`.
CI retains Python 3.10 and Node 20, so its environments differ from this machine.

## Findings and changes

- The previous Xcode license blocker no longer reproduces; no license or system
  configuration was changed during this work.
- The initial ordinary Rust suite passed. The separately run ignored CLI test
  failed on an obsolete `// Content from:` marker despite generating a PDF.
  Assembly fixtures now use embedded helpers and current `BEGIN-FILE` markers.
- CLI assembly tests now run without Typst. Separate ignored PDF tests launch
  the actual `knot build` binary, check a PDF signature and reject invalid Typst.
  The invalid-document test uses unequivocally invalid Typst instead of relying
  on the parser's treatment of an unclosed executable fence.
- Relative traversal paths in the test TOML use forward slashes to avoid invalid
  backslash escapes on Windows.
- Four TypeScript errors came from the untyped `knot/startPreview` result.
  A success/error union now represents that response. Type checking is explicit
  in CI and in the packaging hook.
- Python initially lacked pandas/matplotlib. With dependencies installed, two
  plot tests still failed under the default local graphics environment. Both
  passed with Agg and a writable Matplotlib configuration/cache directory;
  these settings are now explicit in CI. The exact default-backend failure
  was not investigated further in this validation stage.
- Clippy 1.98 flagged the collected thread handles as `needless_collect`.
  An explicit spawn loop retains the required behavior: start every language
  chain before joining any, rather than accepting a lazy iterator rewrite
  that would serialize execution.
- CI now includes CLI tests, interpreter tests and a dedicated Typst step;
  it records versions and uses Cargo's lockfile and the local VSIX packager.
  Typst setup follows the action's documented `typst-version` input:
  <https://github.com/typst-community/setup-typst>.

Cache identity, compilation cancellation/publication, navigation and CLI
behavior changes remain outside this stage.
