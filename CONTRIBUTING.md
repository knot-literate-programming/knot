# Contributing to Knot

Thank you for your interest in contributing to Knot! This document will help you get started with the development environment.

## Project Structure

- `crates/knot-core`: The core engine (parsing, caching, executors).
- `crates/knot-cli`: The command-line interface.
- `crates/knot-lsp`: The Language Server (proxying Tinymist for Typst support).
- `editors/vscode`: The VS Code extension.

## Development Setup

### Prerequisites

- **Rust**: Latest stable version.
- **Node.js & npm**: For VS Code extension development.
- **Typst & Tinymist**: `tinymist` must be available in your PATH or installed via the VS Code extension.
- **R / Python**: (Optional) For testing code chunks.

### Building the Project

```bash
cargo build
```

### Developing the LSP & VS Code Extension

1.  **Compile the LSP**:
    ```bash
    cargo build -p knot-lsp
    ```

2.  **Setup the VS Code extension**:
    ```bash
    cd editors/vscode
    npm install
    npm run compile
    ```

3.  **Run in Debug Mode**:
    - Open the `knot` workspace in VS Code.
    - Press `F5` (or go to "Run and Debug" and select "Launch Extension").
    - This will open a new "Extension Development Host" window.
    - In the settings of this new window, ensure `knot.lsp.path` points to your freshly compiled binary:
      `abs/path/to/knot/target/debug/knot-lsp`.

## Areas for Contribution

We are currently looking for help in the following areas:

- **LSP Improvements**: Mapping Typst diagnostics and navigation (see `crates/knot-lsp/ROADMAP.md`).
- **Sync Mapping**: Implementing the PDF-to-source synchronization (see `SYNC_MAPPING_PLAN.md`).
- **Testing**: Adding integration tests for various R/Python environments.

## Creating a Pull Request

1.  Fork the repository.
2.  Create a feature branch.
3.  Ensure tests pass: `cargo test`.
4.  Submit a PR with a clear description of your changes.
