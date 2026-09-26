# Introduction

Knot is a literate programming system for [Typst](https://typst.app). It lets you
embed executable R and Python code directly inside `.knot` documents, which compile
to `.typ` files that Typst renders into PDF (or any other format Typst supports).

If you have used RMarkdown or Quarto, the idea will feel familiar. Knot's bet is
narrower: **the final document is the notebook**. R and Python compute, Typst
composes, and compilation verifies. The code runs in document order, and what the
PDF shows — results and errors alike — is what that execution produced.

Knot is experimental (0.x): the format, options and configuration may still
change. See [Status and limitations](#status-and-limitations) below.

---

## The Literate Programming Idea

Literate programming, as coined by Donald Knuth, is the practice of writing programs
and their explanations as a single document. The source of truth is the document —
not the code, not the prose, but both together.

In practice this means:

- Your analysis, your methodology, and your conclusions live in the same file as the
  code that produces them.
- The document can be checked: running it again from a clean state reproduces the
  output, as far as the code, its data and its environment are themselves stable.
- There is no "copy the number from the script into the report" step. The number *is*
  the report.

Knot follows this philosophy strictly. A `.knot` document is a Typst document with
executable code blocks. Nothing more, nothing less.

---

## Why Typst?

Typst is a modern typesetting system designed from the ground up to be fast and
programmable. Where LaTeX compilation can take seconds to minutes on a large document,
Typst compiles in milliseconds. Where LaTeX error messages are famously cryptic,
Typst's are precise and helpful.

This speed matters for literate programming. When you fix a typo, you want to see
the result immediately — not wait for a full recompile. Knot exploits Typst's speed
at every level:

- **Static content** (headings, prose, equations) updates in milliseconds.
- **Cached chunks** (code whose output hasn't changed) appear instantly in the preview.
- **Changed chunks** stream into the preview one by one as they finish executing,
  without waiting for the entire document to recompile.

The result is a writing experience where the preview feels live, not batched.

---

## The Execution Model

Understanding how Knot executes code is essential to using it effectively.

### One interpreter per language per file

Each `.knot` file gets its own R interpreter and its own Python interpreter.
Variables defined in one file are **not** visible in another. If your project has
a `chapter1.knot` and a `chapter2.knot`, they are completely isolated — each starts
from a fresh environment.

This isolation is intentional. It enforces modularity and prevents subtle
cross-file dependencies that are hard to debug. If `chapter2.knot` needs a value
computed in `chapter1.knot`, it must read it explicitly (from a file, a database,
or a shared data format).

### Linear execution within a file

Within a single file, code chunks of the same language execute **sequentially**,
in document order. The R chunk on line 50 sees all variables defined by R chunks
above it. The Python chunk on line 200 sees all Python variables defined above it.

R and Python are independent — they do not share a namespace. But within each
language, state accumulates from top to bottom, exactly as if you had run the
file as a script.

```
file: analysis.knot

[R chunk 1]  x <- 1:100        ← defines x
[R chunk 2]  mean(x)           ← sees x ✓
[Python 1]   y = [1, 2, 3]     ← defines y (Python namespace)
[R chunk 3]  sd(x)             ← still sees x ✓
[Python 2]   sum(y)            ← sees y ✓, cannot see x ✗
```

R and Python run **in parallel** when multiple languages are present in the same
file — the R chain and the Python chain are independent and can execute
simultaneously.

---

## The Cache and Invalidation

Re-executing every chunk on every save would be too slow for long documents. Knot
caches the output of every chunk and only re-executes chunks whose inputs have changed.

### Chained hashing

Each chunk's cache key is a SHA-256 hash of:
- The chunk's source code
- Its options (`#|` frontmatter)
- The hash of the **previous chunk** in the same language chain

The chaining is the critical part. If you edit chunk 3, its hash changes. Because
chunk 4's hash includes chunk 3's hash, chunk 4's hash also changes — even if chunk
4's own code is identical. And so does chunk 5's, chunk 6's, and so on.

```
chunk 1 (unchanged)  hash: a1b2c3…
chunk 2 (unchanged)  hash: f(code₂, a1b2c3…) = 9d8e7f…
chunk 3 (EDITED)     hash: f(code₃', 9d8e7f…) = 3c4d5e…  ← changed
chunk 4 (unchanged)  hash: f(code₄, 3c4d5e…) = 7f8a9b…  ← also changed!
chunk 5 (unchanged)  hash: f(code₅, 7f8a9b…) = 2e3f4a…  ← also changed!
```

This cascade is not a bug — it is the correct behaviour. Chunk 4 may depend on a
variable modified by chunk 3. Knot cannot know for certain whether it does, so it
re-executes everything downstream. Reproducibility is guaranteed.

### Environment snapshots

Re-executing chunk 4 requires that the R (or Python) environment be in the same
state it was in *just before* chunk 4 last ran. Knot achieves this through
**environment snapshots**.

After executing each chunk, Knot saves a snapshot of the interpreter's state
(the set of live objects and their values). When a downstream chunk must be
re-executed, Knot restores the snapshot left by the chunk just before it and then
runs the chunk — without having to re-execute all the upstream chunks.

This means that if chunk 5 changes in a 20-chunk document, Knot restores the
snapshot left by chunk 4 and executes chunks 5 to 20 (their cache keys changed
with chunk 5's). Chunks 1 to 4 are served from cache.

---

## Choosing whether to save snapshots

Snapshots accelerate recompilation by restoring the environment before a changed
chunk. By default, Knot saves complete R/Python environments. Large datasets or
models may therefore be repeated in many snapshots, consuming substantial disk
space and serialization time.

Disable snapshots for a language in the YAML header at the very beginning of a
`.knot` file:

```yaml
---
snapshots:
  r: true
  python: false
---
```

Omitted languages default to `true`. With `false`, every compilation executes the
entire language chain in a fresh interpreter, including inline expressions and
unchanged chunks. No snapshots are created or restored for that chain. Correcting
an error also restarts it from the beginning. `eval: false` still skips a node.
Other languages and files keep their own settings and cache.

Snapshots are an optimization, not a guarantee that arbitrary process state can
be reconstructed. R's serialization supports ordinary data and functions, but
external resources such as database connections may not survive restoration.
Python's standard pickle has further limitations, including functions and classes
defined in a chunk. Knot rejects known unusable or damaged snapshots; it cannot
detect every incompatibility. **Set snapshots to `false` when unsure.** This does
not by itself make external inputs or nondeterministic code reproducible.

Split independent analyses into separate `.knot` files to limit memory usage and
the cost of re-execution. Exchange data through files with declared dependencies.
Knot warns in the document when snapshots exceed 1 GB per language and file;
the threshold is configurable. For a complete render without changing your
working settings, use `knot build --no-snapshots`. See [cache and execution
state](./caching.md) for details.

There is no object-level `freeze` option or mutation contract: the policy applies
to the complete language workspace in each file.

---

## The Preview Experience

The VS Code extension brings all of the above together into a fluid writing
experience:

1. **You type** — the preview updates instantly with the current cached output for
   all unchanged chunks. The chunk you edited shows a thick amber dotted border;
   downstream chunks invalidated by the hash cascade show a thin amber dashed border.

2. **You save** — Knot immediately assembles a preview from the cache (cache hits
   in full, chunks that must run shown with an orange border), before running
   any code.

3. **Chunks execute** — as each chunk finishes, its result streams into the preview
   in real time. You see results appear one by one, not all at once at the end.

4. **Sync** — clicking in the PDF scrolls to the corresponding source line.
   Moving your cursor in the source scrolls the PDF. Forward and backward sync
   work bidirectionally.

The goal is to make the feedback loop short enough that you think of writing and
computing as a single activity, not two separate phases.

---

## Comparison with RMarkdown and Quarto

RMarkdown and Quarto are exceptional tools. They have shaped the practice of
reproducible research over many years and inspired much of what Knot tries to do.
If you are already happy with one of them, there is no reason to switch.

Knot is a young project. It does not have the maturity, the ecosystem, or the
community of either. What it offers instead is a narrow but deliberate bet: Typst
as the only typesetting target, with reproducibility as a first-class constraint.

### What RMarkdown and Quarto do better

- **Maturity and ecosystem.** Thousands of packages, templates, and extensions
  have been built around knitr and Quarto. Knot has none of that yet.
- **Output formats.** Quarto targets HTML, Word, presentations, websites, books,
  and more. Knot only produces Typst documents.
- **Language support.** Quarto supports R, Python, Julia, Observable, and others.
  Knot supports R and Python.
- **Typesetting flexibility.** If you need LaTeX — for a journal template, a
  specific package, or a workflow that requires `.tex` output — RMarkdown and
  Quarto are the right tools. Typst is still young and not accepted everywhere.
- **Speed of a first full run.** Knot's incremental model shines on reruns, but
  the first compilation has the same cost as any other tool. On large documents,
  Typst's own compilation is fast; the bottleneck is code execution, which is
  comparable across systems.

### Where Knot makes a different choice

- **The notebook/render split.** RMarkdown and Quarto have two distinct modes.
  In the notebook (interactive) session, execution is non-linear: you can run
  chunks out of order, redefine variables, and accumulate state across runs.
  At render time, the document is executed linearly from a fresh environment.
  This split is a frequent source of frustration: code that worked interactively
  breaks at render because it silently depended on state that no longer exists.
  Knot has only one execution order: the document's. Every compilation runs the
  chunks linearly; the cache and the snapshots only skip work whose inputs have
  not changed, and `knot build --strict --no-snapshots` runs everything from a
  clean state. There is no gap between "works in the session" and "works in the
  document". See [From exploration to publication](./exploration-to-publication.md).
- **Typst instead of LaTeX.** Typst's syntax is clean, its compilation is fast,
  and its layout model is modern. For users who do not need LaTeX compatibility,
  it removes a significant source of friction.
- **Live preview with per-chunk streaming.** Cached chunks appear instantly; only
  invalidated chunks rerun. The preview updates progressively rather than waiting
  for the full document.

| | Knot | RMarkdown | Quarto |
|---|---|---|---|
| Typesetting engine | Typst only | LaTeX / HTML | LaTeX / HTML / others |
| Maturity | Early | Mature | Mature |
| Supported languages | R, Python | R (+ reticulate) | R, Python, Julia, others |
| Output formats | PDF (via Typst) | Many | Many |
| Execution order | Always linear | Linear at render, non-linear interactively | Linear at render, non-linear interactively |
| Caching | Chained SHA-256 | knitr cache (per chunk) | Freeze / cache |
| Live preview | Streaming, per-chunk | Re-knit | Re-render on save |
| Bidirectional sync | Yes | No | Partial |

---

## Status and limitations

Knot is experimental (0.x). Known limitations of the current version:

- Output is explicit: plots appear through `typst(p)`, `base_plot({ ... })` (R)
  or `typst(current_plot())` (Python), and a Python chunk does not display its
  last expression. A chunk shows at most one plot and one table, not in emission
  order.
- Data frames are rendered as plain tables; for publication, export the data
  with `export_data` and compose the table in Typst.
- Snapshots are an optimisation: some objects cannot be restored. Knot does not
  record package versions or pin the environment.
- R and Python only; PDF output only, through Typst; editor integration for VS
  Code only.
