# Contributing to Knot

Thank you for your interest in contributing to Knot! 🎉

Knot is in **early development** (v0.1.x), and we welcome testers, bug reporters, and code contributors.

---

## Ways to Contribute

### 🐛 Report Bugs

Found a bug? [Open an issue](https://github.com/knot-literate-programming/knot/issues/new) with:
- **Description**: What happened vs what you expected
- **Steps to reproduce**: Minimal example to trigger the bug
- **Environment**: OS, Rust version, R/Python version
- **Logs**: Any error messages or stack traces

### 💡 Suggest Features

Have an idea? [Open an issue](https://github.com/knot-literate-programming/knot/issues/new) with:
- **Use case**: What problem does this solve?
- **Proposed solution**: How should it work?
- **Alternatives**: Other approaches you considered

### 📝 Improve Documentation

- Fix typos or unclear explanations
- Add examples or tutorials
- Update outdated information

### 💻 Contribute Code

See **Development Setup** below to get started.

---

## Development Setup

### Prerequisites

- **Rust** 1.70+ — [Install from rustup.rs](https://rustup.rs/)
- **Node.js** 18+ — [Install from nodejs.org](https://nodejs.org/)
- **Typst CLI** — [Install from typst.app](https://github.com/typst/typst#installation)
- **R** (optional) — For testing R executor
- **Python 3.8+** (optional) — For testing Python executor

### Clone the repository

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot
```

### Build the project

```bash
# Build all components
cargo build --release

# Binaries will be in target/release/
# - knot
# - knot-lsp
```

### Run tests

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p knot-core
cargo test -p knot-lsp
```

### Build the VS Code extension

```bash
cd editors/vscode
npm install
npm run compile

# Package extension
npm run package
# Creates knot-*.vsix
```

---

## Project Structure

```
knot/
├── crates/
│   ├── knot-core/          # Parser, executor, cache engine
│   │   ├── src/
│   │   │   ├── parser/     # Winnow-based parser
│   │   │   ├── executors/  # R and Python executors
│   │   │   ├── compiler/   # Compilation pipeline
│   │   │   └── cache/      # SHA256-based caching
│   │   └── resources/      # Embedded helper scripts (typst.py, typst.R)
│   ├── knot-cli/           # Command-line interface
│   └── knot-lsp/           # Language Server Protocol
├── editors/
│   └── vscode/             # VS Code extension
├── templates/
│   └── minimal/            # knot init template
├── examples/               # Example projects
└── docs/
    └── dev-plans/          # Architecture and design docs
```

---

## Development Workflow

### 1. Pick an issue

Browse [issues labeled "good first issue"](https://github.com/knot-literate-programming/knot/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).

Comment on the issue to claim it and avoid duplicate work.

### 2. Create a branch

```bash
git checkout -b fix/issue-description
# or
git checkout -b feature/feature-name
```

### 3. Make changes

- **Write tests** for new functionality
- **Update documentation** if behavior changes
- **Follow Rust conventions** (rustfmt, clippy)

### 4. Test your changes

```bash
# Format code
cargo fmt --all

# Check for issues
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --workspace
```

### 5. Commit

```bash
git add .
git commit -m "fix: description of the fix

Longer explanation if needed.

Fixes #123"
```

**Commit message format:**
- `fix:` — Bug fixes
- `feat:` — New features
- `docs:` — Documentation changes
- `refactor:` — Code refactoring
- `test:` — Test additions or fixes
- `ci:` — CI/CD changes

### 6. Push and create a PR

```bash
git push origin your-branch-name
```

Open a Pull Request on GitHub with:
- **Description**: What does this PR do?
- **Related issues**: `Fixes #123` or `Relates to #456`
- **Testing**: How did you test this?

---

## Code Style

### Rust

We follow standard Rust conventions:

- **Format with rustfmt**: `cargo fmt --all`
- **Lint with clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
- **Document public APIs**: Use `///` doc comments
- **Write tests**: Unit tests in the same file, integration tests in `tests/`

### TypeScript (VS Code extension)

- **Use ESLint**: Configured in `editors/vscode/.eslintrc.json`
- **Format with Prettier**: Run `npm run lint`

---

## Testing

### Unit tests

```bash
# Test a specific crate
cargo test -p knot-core

# Test a specific module
cargo test -p knot-core parser
```

### Integration tests

```bash
# Test the full compilation pipeline
cargo test -p knot-cli --test integration_build
```

### Manual testing

```bash
# Build and test with an example
cargo build --release
./target/release/knot compile examples/demo-python/main.knot
typst compile examples/demo-python/.main.typ test.pdf
```

---

## Architecture Guidelines

### Parser (`knot-core/src/parser`)

- Uses **winnow** for parsing
- AST defined in `ast.rs`
- Chunk options defined with `define_options!` macro

### Executors (`knot-core/src/executors`)

- **Persistent processes**: One R/Python process per session
- **Side-channel communication**: Metadata via JSON files
- Helper scripts embedded in binary (`resources/typst.py`, `typst.R`)

### Compiler (`knot-core/src/compiler`)

- Sequential chunk execution (deterministic)
- Dependency tracking for caching
- Generates pure Typst output

### LSP (`knot-lsp`)

- Proxies Typst LSP to Tinymist
- Adds Knot-specific features (hover on chunks, chunk option completion)
- Position mapping between `.knot` and `.typ` coordinates

---

## Design Principles

1. **Reproducibility**: Execution is strictly linear and deterministic
2. **Performance**: Rust for speed, Typst for fast PDF generation
3. **Simplicity**: Typst is the ONLY documentation language (no Markdown intermediate)
4. **Caching**: SHA256-based, cascading invalidation
5. **IDE integration**: First-class support via LSP

---

## Getting Help

- **Questions?** Open a [discussion](https://github.com/knot-literate-programming/knot/discussions)
- **Stuck?** Comment on your PR or issue
- **Architecture questions?** Read [docs/dev-plans/](docs/dev-plans/)

---

## Code of Conduct

Be respectful, inclusive, and constructive. We're all here to build something cool together.

---

**Thank you for contributing!** 🙏
