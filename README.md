# Knot

**knot is not knitr** — literate programming for [Typst](https://typst.app/), powered by Rust.

[![CI](https://github.com/knot-literate-programming/knot/actions/workflows/ci.yml/badge.svg)](https://github.com/knot-literate-programming/knot/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/knot-literate-programming/knot)](https://github.com/knot-literate-programming/knot/releases)
[![Documentation](https://img.shields.io/badge/docs-knot--literate--programming.github.io%2Fknot-blue)](https://knot-literate-programming.github.io/knot/)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

**The final document is the notebook.** You write one Typst document with
executable R and Python code. R and Python compute, Typst composes, and
compilation verifies. The code runs in document order: what you read in the PDF
is what was executed, errors included, shown where they occur.

<!-- TODO(#2): add demo GIF here -->

## A scientific report, end to end

The [Anscombe example](examples/anscombe) analyses one versioned dataset three
ways — in R with ggplot2, in Python with plotnine, and split between the two
languages, drawn in Typst with Gribouille — and checks during compilation that
all three agree. [Read the PDF](docs/book/src/assets/anscombe.pdf) (five pages).

<p align="center">
  <img src="docs/book/src/images/anscombe-mixed.png" width="640"
       alt="Series I–II computed in R, III–IV in Python, one table and one figure drawn in Typst">
</p>

<p align="center">
  <img src="docs/book/src/images/anscombe-validation.png" width="640"
       alt="The cross-language check, executed during PDF compilation">
</p>

## Two modes, one document

- **Exploration.** In VS Code, the preview updates as you type, from the cache.
  On save, only the chunks that changed — and those after them in the same
  language — run again, restarting from a snapshot of the interpreter state.
  Results appear as they are computed.
- **Publication.** `knot build --strict --no-snapshots` re-executes everything
  from a fresh interpreter and fails on any error: the PDF you publish is the
  one a clean run produces.

See [From exploration to publication](docs/book/src/exploration-to-publication.md).

## Features

- **R and Python** in the same document, each language running in its own
  interpreter, in parallel
- **Linear execution** — chunks of a language run in document order; a chained
  SHA-256 cache re-executes an edited chunk and everything after it
- **Errors in the PDF** — runtime, option and configuration errors are rendered
  where they occur, and reported in the editor with the same message
- **Rich output** — plots (SVG/PNG), data frames as Typst tables, inline values,
  and data exported to Typst for your own tables and figures
- **Live preview** in VS Code, with bidirectional source ↔ PDF sync
- **IDE support** — completion, hover docs, diagnostics, formatting (Air, Ruff, Typstyle)

## Status and limitations

Knot is **experimental** (0.x): the document format, options and configuration
may still change. Known limitations:

- Output is explicit: plots appear through `typst(p)`, `base_plot({ ... })` (R)
  or `typst(current_plot())` (Python), and a Python chunk does not display its
  last expression. A chunk shows at most one plot and one table, not in emission
  order.
- Data frames are rendered as plain tables; for publication, export the data and
  compose the table in Typst.
- Snapshots are an optimisation: some objects cannot be restored (connections,
  Python functions and classes defined in a chunk). Knot does not record package
  versions or pin the environment.
- R and Python only; PDF output only, through Typst; editor integration for VS
  Code only.

---

## Installation

**macOS / Linux**
```bash
curl -sSf https://raw.githubusercontent.com/knot-literate-programming/knot/master/install.sh | bash
```

**Windows (PowerShell)**
```powershell
powershell -c "irm https://github.com/knot-literate-programming/knot/releases/latest/download/knot-installer.ps1 | iex"
```

The installers download the prebuilt `knot` and `knot-lsp` binaries for your platform. The macOS/Linux script also installs the VS Code extension and checks that all prerequisites are in place; on Windows, download the `.vsix` from the [latest release](https://github.com/knot-literate-programming/knot/releases) and install it with `code --install-extension knot-*.vsix`.

**Prerequisites:**

| Tool | Purpose | Required? |
|---|---|---|
| [Typst](https://github.com/typst/typst/releases) | Compiles `.typ` → PDF | Yes |
| [Tinymist](https://marketplace.visualstudio.com/items?itemName=myriad-dreamin.tinymist) | Powers the VS Code preview (the extension is enough) | Yes |
| [R](https://cran.r-project.org) | Executes R chunks, with the packages `jsonlite`, `digest` and `svglite` | If using R |
| [Python](https://www.python.org/downloads) | Executes Python chunks (plus the libraries you use, e.g. matplotlib) | If using Python |
| [Air](https://posit-dev.github.io/air) | R code formatter in VS Code | Recommended |
| [Ruff](https://docs.astral.sh/ruff/installation) | Python code formatter in VS Code | Recommended |

<details>
<summary>Build from source</summary>

```bash
git clone https://github.com/knot-literate-programming/knot.git
cd knot
bash scripts/install-dev.sh
```

This rebuilds and installs both Rust binaries and the VS Code extension.
Run **Developer: Reload Window** in VS Code afterwards. Rust, Node.js and the
`code` command must be available. For CLI-only installation, see the
[installation guide](docs/book/src/installation.md#build-from-source).
</details>

---

## Quick Start

```bash
knot init my-project
cd my-project
code .           # open main.knot, then click "Open Preview" in the editor title bar
```

A `.knot` file is a Typst document with executable code blocks. Below the
template's codly configuration in `main.knot`, write:

~~~typst
= My Analysis

```{r summary}
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
knot build       # one-shot PDF: main.pdf
knot watch       # rebuild on save
```

---

## How It Works

Knot compiles `.knot` files in three passes:

1. **Plan** — parse, hash each chunk (chained with the previous one of its
   language) and decide what is cached and what must run
2. **Execute** — run the R and Python chains in parallel, each in document order
3. **Assemble** — interleave the outputs with the prose into a `.typ` file, which
   Typst compiles to PDF

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
  version = {0.3.2},
  year    = {2026}
}
```

See [CITATION.cff](CITATION.cff) for full metadata.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).
Inspired by [knitr](https://yihui.org/knitr/), [Quarto](https://quarto.org/), and [Typst](https://typst.app/).
