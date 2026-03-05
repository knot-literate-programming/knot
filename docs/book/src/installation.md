# Installation

## Quick install

```bash
curl -sSf https://raw.githubusercontent.com/knot-literate-programming/knot/master/install.sh | bash
```

This script:
1. Detects your platform (macOS arm64/x86_64, Linux x86_64/arm64).
2. Downloads the prebuilt `knot` and `knot-lsp` binaries from the [latest release](https://github.com/knot-literate-programming/knot/releases).
3. Installs them to `~/.local/bin` (override with `--prefix DIR`).
4. Installs the VS Code extension if the `code` command is available.
5. Checks that all prerequisites are present and tells you what is missing.

## Prerequisites

### Required

| Tool | Purpose | Install |
|---|---|---|
| [Typst](https://github.com/typst/typst/releases) | Compiles `.typ` → PDF | See below |
| [Tinymist](https://github.com/Myriad-Dreamin/tinymist/releases) | Powers the VS Code live preview | See below |

**Typst** can be installed with most package managers:
```bash
# macOS
brew install typst

# cargo
cargo install --locked typst-cli
```

**Tinymist** is the Typst language server. Download the binary for your platform
from the [Tinymist releases page](https://github.com/Myriad-Dreamin/tinymist/releases)
and place it somewhere in your `PATH`.

> **Note:** The Tinymist VS Code extension bundles its own binary, but Knot's LSP
> spawns a *separate* Tinymist subprocess and needs the binary available in `PATH`
> independently.

### Per language (install what you use)

| Tool | Purpose | Install |
|---|---|---|
| [R](https://cran.r-project.org) | Execute R chunks | CRAN |
| [Air](https://posit-dev.github.io/air) | Format R code in VS Code | See Air docs |
| [Python 3](https://www.python.org/downloads) | Execute Python chunks | python.org, conda, pyenv… |
| [Ruff](https://docs.astral.sh/ruff/installation) | Format Python code in VS Code | `pip install ruff` |

You only need the tools for the languages you actually use. If your document has
no R chunks, you do not need R.

## Build from source

If there is no prebuilt binary for your platform, or if you want to work from the
latest development version:

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot

# Install the CLI and LSP
cargo install --path crates/knot-cli
cargo install --path crates/knot-lsp

# Install the VS Code extension
bash scripts/install-vscode-dev.sh
```

You need [Rust](https://rustup.rs) 1.80+ and [Node.js](https://nodejs.org) 20+.

## Verifying the installation

```bash
knot --version
knot-lsp --version
typst --version
tinymist --version
```

All four commands should print a version string without errors.
