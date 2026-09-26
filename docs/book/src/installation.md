# Installation

## Quick install

**macOS / Linux**

```bash
curl -sSf https://raw.githubusercontent.com/knot-literate-programming/knot/master/install.sh | bash
```

This script:
1. Detects your platform (macOS arm64/x86_64, Linux x86_64/arm64).
2. Downloads the prebuilt `knot` and `knot-lsp` binaries from the [latest release](https://github.com/knot-literate-programming/knot/releases).
3. Installs them to `~/.local/bin` (override with `--prefix DIR`).
4. Installs the VS Code extension if the `code` command is available.
5. Checks that all prerequisites are present and tells you what is missing.

**Windows (PowerShell)**

```powershell
powershell -c "irm https://github.com/knot-literate-programming/knot/releases/latest/download/knot-installer.ps1 | iex"
```

This installs `knot` and `knot-lsp` to `%USERPROFILE%\.cargo\bin` and adds it to your `PATH`. Then install the VS Code extension manually: download the `.vsix` file from the [latest release](https://github.com/knot-literate-programming/knot/releases) and run:

```powershell
code --install-extension knot-*.vsix
```

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

**Tinymist** is the Typst language server. In VS Code, installing the
[Tinymist extension](https://marketplace.visualstudio.com/items?itemName=myriad-dreamin.tinymist)
is enough: Knot's language server runs its own Tinymist process from the binary
bundled with that extension. `knot watch --preview` and other editors need the
`tinymist` binary on your `PATH` (from the
[Tinymist releases page](https://github.com/Myriad-Dreamin/tinymist/releases)) or
selected with `[tools] tinymist` in `knot.toml`.

### Per language (install what you use)

| Tool | Purpose | Install |
|---|---|---|
| [R](https://cran.r-project.org) | Execute R chunks | CRAN |
| [Air](https://posit-dev.github.io/air) | Format R code in VS Code | See Air docs |
| [Python 3](https://www.python.org/downloads) | Execute Python chunks | python.org, conda, pyenv… |
| [Ruff](https://docs.astral.sh/ruff/installation) | Format Python code in VS Code | `pip install ruff` |

You only need the tools for the languages you actually use. If your document has
no R chunks, you do not need R.

Knot's R helpers use three CRAN packages: `jsonlite` (every R chunk),
`digest` (figures, tables and data exports) and `svglite` (SVG figures):

```r
install.packages(c("jsonlite", "digest", "svglite"))
```

Python needs no package for Knot itself; install the libraries your chunks use,
for example `matplotlib` for figures and `pandas` for data frames.

## Build from source

If there is no prebuilt binary for your platform, or if you want to work from the
latest development version:

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot

# Install the CLI, LSP and VS Code extension together
bash scripts/install-dev.sh
```

The script rebuilds and replaces both Rust binaries from this checkout (even
when their version number is unchanged), then installs the extension using the
locked npm dependencies. Cargo uses its configured installation directory,
usually `~/.cargo/bin`. The `cargo`, `node`, `npm` and `code` commands must be
available on `PATH`. Run **Developer: Reload Window** in VS Code afterwards to
restart the LSP. If you customized `knot.lsp.path`, point it to the updated binary.

For CLI/LSP installation without VS Code:

```bash
cargo install --locked --force --path crates/knot-cli
cargo install --locked --force --path crates/knot-lsp
```

You need [Rust](https://rustup.rs) 1.92+ and [Node.js](https://nodejs.org) 22+.

## Verifying the installation

```bash
knot --version
knot-lsp --version
typst --version
tinymist --version
```

All four commands should print a version string without errors.
