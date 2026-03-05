# Knot

**knot is not knitr** — Literate programming for [Typst](https://typst.app/), powered by Rust.

[![CI](https://github.com/knot-literate-programming/knot/actions/workflows/ci.yml/badge.svg)](https://github.com/knot-literate-programming/knot/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/knot-literate-programming/knot)](https://github.com/knot-literate-programming/knot/releases)
[![Documentation](https://img.shields.io/badge/docs-knot--literate--programming.github.io%2Fknot-blue)](https://knot-literate-programming.github.io/knot/)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

Knot lets you write R and Python code directly inside Typst documents. The code executes, and the results — text, plots, tables — flow into your document automatically. Think RMarkdown or Quarto, but with Typst instead of LaTeX and a Rust engine underneath.

<!-- TODO: add demo GIF here -->

---

## Features

- **Multi-language** — R and Python in the same document, running in parallel
- **Smart caching** — SHA-256 chained hashes; only changed chunks re-execute
- **Live preview** — chunk-by-chunk streaming preview in VS Code as code runs
- **Rich output** — plots (SVG/PNG), DataFrames as Typst tables, inline expressions
- **Bidirectional sync** — click in the PDF to jump to the source, and vice versa
- **Full IDE support** — completion, hover docs, diagnostics, formatting (Air + Ruff + Tinymist)

---

## Installation

```bash
curl -sSf https://raw.githubusercontent.com/knot-literate-programming/knot/master/install.sh | bash
```

The script downloads the prebuilt binaries for your platform, installs the VS Code extension, and checks that all prerequisites (Typst, Tinymist, R, Python, Air, Ruff) are in place.

**Prerequisites:**

| Tool | Purpose | Required? |
|---|---|---|
| [Typst](https://github.com/typst/typst/releases) | Compiles `.typ` → PDF | Yes |
| [Tinymist](https://github.com/Myriad-Dreamin/tinymist/releases) | Powers the VS Code preview | Yes |
| [R](https://cran.r-project.org) | Executes R chunks | If using R |
| [Python](https://www.python.org/downloads) | Executes Python chunks | If using Python |
| [Air](https://posit-dev.github.io/air) | R code formatter in VS Code | Recommended |
| [Ruff](https://docs.astral.sh/ruff/installation) | Python code formatter in VS Code | Recommended |

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot
cargo install --path crates/knot-cli
cargo install --path crates/knot-lsp
```

Then install the VS Code extension:
```bash
cd editors/vscode && npm install && npm run package
code --install-extension knot-*.vsix
```
</details>

---

## Quick Start

```bash
knot init my-project
cd my-project
code .           # open in VS Code, then click "Start Preview"
```

A `.knot` file is a Typst document with executable code blocks:

~~~typst
#import "lib/knot.typ": *

= My Analysis

```{r}
#| label: summary
x <- c(2, 4, 6, 8, 10)
summary(x)
```

The mean is `{r} mean(x)` and the standard deviation is `{r} round(sd(x), 2)`.

```{python}
import matplotlib.pyplot as plt
plt.plot([1, 4, 9, 16])
plt.title("Growth")
typst(current_plot())
```
~~~

Compile to PDF:
```bash
knot build       # one-shot PDF
knot watch       # rebuild on save
```

---

## How It Works

Knot compiles `.knot` files through a three-pass pipeline:

1. **Plan** — parse, compute SHA-256 hashes, classify each chunk as `Skip` / `CacheHit` / `MustExecute`
2. **Execute** — run R and Python chains in parallel; cache results
3. **Assemble** — interleave outputs with source text to produce a `.typ` file for Typst

The VS Code extension shows a streaming preview: cache hits appear instantly, and each chunk updates the preview as it finishes executing.

---

## Documentation

**[knot-literate-programming.github.io/knot](https://knot-literate-programming.github.io/knot)**

- User guide — installation, chunk options, VS Code, CLI reference
- Developer guide — architecture, adding a language executor, LSP internals

---

## Contributing

We welcome bug reports, feature requests, and pull requests. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and guidelines.

---

## Citing Knot

```bibtex
@software{Klutchnikoff_Knot_2026,
  author  = {Klutchnikoff, Nicolas},
  title   = {{Knot: Literate programming for Typst}},
  url     = {https://github.com/knot-literate-programming/knot},
  version = {0.3.0},
  year    = {2026}
}
```

See [CITATION.cff](CITATION.cff) for full metadata.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).
Inspired by [knitr](https://yihui.org/knitr/), [Quarto](https://quarto.org/), and [Typst](https://typst.app/).
