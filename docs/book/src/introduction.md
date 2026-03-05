# Introduction

Knot is a literate programming system for [Typst](https://typst.app). It lets you
embed executable R and Python code directly inside `.knot` documents, which compile
to `.typ` files that Typst renders into PDF (or any other format Typst supports).

If you have used RMarkdown or Quarto, the idea will feel familiar. The differences
are in the details — and the details matter.

---

## The Literate Programming Idea

Literate programming, as coined by Donald Knuth, is the practice of writing programs
and their explanations as a single document. The source of truth is the document —
not the code, not the prose, but both together.

In practice this means:

- Your analysis, your methodology, and your conclusions live in the same file as the
  code that produces them.
- The document is always reproducible: running it again produces exactly the same output.
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

Before executing each chunk, Knot saves a snapshot of the interpreter's state
(the set of live objects and their values). When a downstream chunk must be
re-executed, Knot restores the snapshot from just before that chunk and then
runs the chunk — without having to re-execute all the upstream chunks.

This means that if only chunk 5 changes in a 20-chunk document, Knot restores
the snapshot from before chunk 5 and executes only chunk 5. The other 19 chunks
are served from cache.

---

## The Freeze Contract

### The snapshot trade-off

Snapshots make incremental recompilation fast, but they have a cost: each snapshot
captures the **entire interpreter environment** at that point — every variable,
every object. If your document loads a multi-gigabyte dataset or trains a model,
that object is included in every subsequent snapshot. With twenty chunks after the
training step, you end up with twenty copies of the model on disk, and restoring
any one of those snapshots means reading the full file back into memory.

This is the core trade-off:

| | Default (no freeze) | With `freeze` |
|---|---|---|
| **Snapshots** | Heavy — large objects included in each one | Light — large objects excluded |
| **Disk usage** | Grows with number of downstream chunks | Object stored once |
| **Snapshot restore** | One large file to read | Small snapshot + separate object load |
| **Constraint** | None | Object must not be mutated downstream |

### How freeze works

~~~typst
```{r}
#| freeze: [model, training_data]
model <- train(training_data)
```
~~~

When you declare `freeze: [model, training_data]`, Knot:

1. **Serialises** each named object into content-addressed storage
   (`.knot_cache/objects/{hash}.ext`) immediately after the chunk executes.
2. **Excludes** those objects from all subsequent snapshots, keeping them
   lightweight.
3. **Reloads** the objects separately after every snapshot restore — so the
   interpreter always has them available.

The objects are still serialised and deserialised; the gain is that snapshots
themselves stay small and fast to write and read, and the objects live on disk
only once regardless of how many chunks follow.

### The immutability contract

In exchange, Knot computes an [xxHash64](https://xxhash.com/) fingerprint of each
frozen object immediately after declaration. After every subsequent chunk in the
same language chain that must re-execute, Knot recomputes the fingerprints and
compares them against the stored values. If they differ — meaning some downstream
code accidentally modified them — Knot marks the violation and suspends execution
of the rest of the chain, surfacing the error in the preview and in VS Code
diagnostics.

This gives you a **compile-time contract**: "these objects must not change after
this point." If they do, you find out immediately, not after you have published the
document.

xxHash64 was chosen for its speed: it can fingerprint hundreds of megabytes per
second, making it practical even for large in-memory objects.

### When to use freeze

Use `freeze` when an object is **large and immutable** after its creation chunk —
a trained model, a loaded dataset, a precomputed matrix. Do not use it for objects
that downstream chunks are expected to modify.

---

## The Preview Experience

The VS Code extension brings all of the above together into a fluid writing
experience:

1. **You type** — the preview updates instantly with the current cached output for
   all unchanged chunks. The chunk you edited shows a thick amber dotted border;
   downstream chunks invalidated by the hash cascade show a thin amber dashed border.

2. **You save** — Knot immediately assembles a Phase 0 preview (cache hits in full,
   pending chunks shown with an orange border). This appears in under 50 ms.

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

- **Notebook-style execution is not reproducible by default.** In RMarkdown and
  Quarto, nothing prevents a notebook session from accumulating state across
  interactive runs. A chunk can depend on an object defined two sessions ago and
  the document will still compile — until it does not. Knot enforces a strictly
  linear execution order and a chained cache: every chunk depends on everything
  before it. Reproducibility is structural, not a convention.
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
| Execution order | Strictly linear | Notebook (non-deterministic) | Notebook (non-deterministic) |
| Caching | Chained SHA-256 | knitr cache (per chunk) | Freeze / cache |
| Live preview | Streaming, per-chunk | None / slow | Limited |
| Bidirectional sync | Yes | No | Partial |
