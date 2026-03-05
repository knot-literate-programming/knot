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

For large objects — a trained model, a multi-gigabyte dataset — serialising and
restoring a snapshot may itself be expensive. The `freeze` option addresses this.

```typst
```{r}
#| freeze: [model, training_data]
model <- train(training_data)
```
```

When you declare `freeze: [model, training_data]`, Knot computes an
[xxHash64](https://xxhash.com/) fingerprint of each named object immediately after
the chunk executes and stores it in the cache.

After every subsequent chunk in the same language chain that must re-execute, Knot
recomputes the fingerprints of `model` and `training_data` and compares them against
the stored values. If they differ — meaning some downstream code accidentally
modified them — Knot marks the violation and suspends execution of the rest of
the chain, surfacing the error in the preview and in VS Code diagnostics.

This gives you a **compile-time contract**: "these objects must not change after
this point." If they do, you find out immediately, not after you have published the
document.

xxHash64 was chosen for its speed: it can fingerprint hundreds of megabytes per
second, making it practical even for large in-memory objects.

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

| | Knot | RMarkdown | Quarto |
|---|---|---|---|
| Typesetting engine | Typst | LaTeX / HTML | LaTeX / HTML / others |
| Engine language | Rust | R | Rust |
| Supported languages | R, Python | R (+ others via reticulate) | R, Python, Julia, others |
| Caching | Chained SHA-256 | knitr cache (by chunk) | Freeze/cache |
| Live preview | Streaming, per-chunk | None / slow | Limited |
| Bidirectional sync | Yes | No | Partial |
| Cross-file isolation | Strict | No isolation | No isolation |

Knot's scope is deliberately narrower than Quarto's. It does not try to target
every output format or every language. It targets Typst, R, and Python — and tries
to do that combination exceptionally well.
