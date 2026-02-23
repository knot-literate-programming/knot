# Contributing to Knot

Thank you for your interest in contributing! 🧶

Knot is an open-source project, and we welcome contributions in many forms: reporting bugs, suggesting features, improving documentation, or contributing code.

---

## Code of Conduct

Be respectful, inclusive, and constructive. We follow the common standards of the Rust and Typst communities.

---

## Development Setup

### Prerequisites
- **Rust** 1.80+ — [Install from rustup.rs](https://rustup.rs/)
- **Node.js** 20+ — For building the VS Code extension
- **Typst CLI** — For document rendering
- **R** and **Python 3.8+** — For testing executors

### Clone the repository
```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot
```

### Build the project
```bash
# Build all components
cargo build --release

# Binaries: target/release/knot and target/release/knot-lsp
```

### Build the VS Code extension
```bash
cd editors/vscode
npm install
npm run compile
# Create .vsix: npm run package
```

---

## Development Workflow

1.  **Pick an issue** or [open one](https://github.com/knot-literate-programming/knot/issues/new) to discuss a feature.
2.  **Create a branch**: `git checkout -b fix/issue-description` or `git checkout -b feature/feature-name`.
3.  **Implement changes** and add tests if applicable.
4.  **Format and lint**:
    ```bash
    cargo fmt --all
    cargo clippy --all-targets --all-features -- -D warnings
    ```
5.  **Run tests**: `cargo test --workspace`.
6.  **Push and create a Pull Request**.

**Commit messages** should follow the Conventional Commits format (e.g., `feat: ...`, `fix: ...`, `docs: ...`).

---

## Project Architecture

- **`crates/knot-core`**: The heart of Knot. Winnow-based parser, SHA256 caching, and persistent executors for R and Python.
- **`crates/knot-cli`**: The user-facing command-line tool.
- **`crates/knot-lsp`**: Language Server implementation. Proxies Typst LSP (Tinymist) while adding Knot-specific overlays.
- **`editors/vscode`**: VS Code extension source.

---

## Documentation

For high-level roadmap and architectural details, see the documents in **[docs/dev-plans/](docs/dev-plans/)**.

---

**Thank you for your help in making Knot the best literate programming system for Typst!** 🙏
