# Knot for VS Code

Language support for Knot files (`.knot`) - Literate programming with R and Typst.

## Features

- **Syntax highlighting** for Knot documents
  - Typst markup highlighting
  - Embedded R code highlighting in chunks
  - Inline R expression highlighting
  - Chunk options highlighting (`#| eval: false`)

- **Code formatting** with Air
  - Format R chunks on type (automatic on newline)
  - Format R chunks on save
  - Format entire document

- **Language Server** integration (upcoming)
  - Diagnostics for parsing errors
  - Document symbols (chunk navigation)
  - Hover information
  - Completion for chunk options

## Installation

### From GitHub Releases (Recommended for testing)

1. Download the latest `.vsix` file from [GitHub Releases](https://github.com/knot-literate-programming/knot/releases)
2. Install in VS Code:
   ```bash
   code --install-extension knot-0.1.0.vsix
   ```
   Or via VS Code UI: `Extensions` → `⋯` → `Install from VSIX...`

### From VS Code Marketplace (Coming soon)

Search for "Knot" in the VS Code Extensions marketplace.

## Requirements

- **Air** formatter for R code formatting (optional but recommended)
  - Install: `curl -fsSL https://posit-dev.github.io/air/install.sh | sh`
  - Or via Homebrew: `brew install posit-dev/tap/air`

- **knot-lsp** for language server features (optional)
  - Download from [GitHub Releases](https://github.com/knot-literate-programming/knot/releases)
  - Or build from source: `cargo build --release -p knot-lsp`

## Extension Settings

This extension contributes the following settings:

- `knot.lsp.enabled`: Enable/disable Knot Language Server
- `knot.lsp.path`: Path to knot-lsp executable
- `knot.formatter.air.path`: Path to Air formatter executable
- `knot.formatter.formatOnType`: Format R chunks automatically on new line
- `knot.formatter.formatOnSave`: Format R chunks automatically on save

## Usage

1. Open any `.knot` file
2. Syntax highlighting is automatic
3. Format with `Shift+Alt+F` (or `Cmd+Shift+P` → "Format Document")
4. Format on type is enabled by default (formats R chunks on newline)

## Known Issues

- Full LSP features (diagnostics, hover, completion) require knot-lsp (in development)
- Typst syntax highlighting is basic (install Typst extension for full support)

## Release Notes

### 0.1.0

Initial release:
- Syntax highlighting for Knot documents
- R code formatting with Air
- Basic Typst markup support
