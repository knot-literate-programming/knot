# Contributing

## Development setup

### Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| Rust | 1.80+ | Build all crates |
| Node.js | 20+ | Build the VS Code extension |
| Typst | latest | Document rendering in tests |
| Tinymist | latest | Needed by the LSP |
| R | 4.0+ | Run R executor tests |
| Python | 3.8+ | Run Python executor tests |

R and Python are optional for unit tests but required for integration tests.

### Clone and build

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot
cargo build --release
```

Binaries: `target/release/knot` and `target/release/knot-lsp`.

### Build the VS Code extension

```bash
cd editors/vscode
npm install
npm run compile
```

Install a development build into VS Code:

```bash
bash scripts/install-vscode-dev.sh
```

This packages the extension into a `.vsix` file and installs it via
`code --install-extension`. Restart VS Code to activate.

---

## Before opening a PR

Run the same checks as CI, in this order:

```bash
# 1. Check & Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# 2. Tests (excluding knot-cli integration tests)
cargo test --workspace --exclude knot-cli

# 3. VS Code extension
cd editors/vscode && npm ci && npm run compile
```

All three must pass with no warnings before pushing.

---

## Commit conventions

Knot uses [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: short description of the new feature
fix: description of the bug that was fixed
docs: documentation change
refactor: code change without behaviour change
test: add or change tests
chore: build system, dependencies, CI
```

Scope is optional but helpful for larger PRs:

```
feat(lsp): add Go to Definition for Typst symbols
fix(executor): set KNOT_FIG_FORMAT env var in Python process
```

---

## Project structure recap

```
crates/
  knot-core/
    src/
      parser/          # Winnow-based .knot parser
      compiler/        # Three-pass pipeline
        pipeline.rs    # Pass 1: planning + hashing
        execution.rs   # Pass 2: parallel execution
        mod.rs         # Pass 3: assembly + two-phase API
        freeze.rs      # Freeze contract checking
        snapshot_manager.rs
        node_output.rs
        options.rs
        sync.rs        # #KNOT-SYNC markers
      executors/       # R and Python subprocesses
      cache/           # SHA-256 addressed persistent cache
      backend.rs       # Node → .typ text rendering
      project.rs       # Top-level compile_project_* API
      config.rs        # knot.toml parsing
  knot-cli/
    src/main.rs        # CLI command dispatch
  knot-lsp/
    src/
      server_impl.rs   # Request/notification handlers
      state.rs         # ServerState (Arc<RwLock<>>)
      proxy.rs         # Forward to Tinymist
      position_mapper.rs
      diagnostics.rs
      handlers/        # Completion, hover, formatting
      sync.rs          # Forward/backward sync
editors/
  vscode/
    src/
      extension.ts     # Activation, preview lifecycle
      projectExplorer.ts
resources/
  typst.R              # Embedded R helper (typst(), current_plot())
  typst.py             # Embedded Python helper
```

---

## Adding a new language executor

See [Language Executors](./dev-executors.md) for the complete checklist.

## Adding a chunk option

See [Adding a Chunk Option](./dev-chunk-option.md) for the complete walkthrough.

---

## Releasing

Releases are automated via `cargo-dist`. Pushing a version tag triggers GitHub
Actions to build binaries for all supported platforms and publish a GitHub
Release with the binaries and the VS Code `.vsix`.

```bash
# Bump version, tag, and push (triggers CI + release)
git tag v0.4.0
git push origin v0.4.0
```
