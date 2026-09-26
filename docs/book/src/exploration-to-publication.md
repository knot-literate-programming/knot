# From exploration to publication

A Knot document is used in two ways: while you explore, you want immediate
feedback; when you publish, you want a PDF that a clean run of the document
produces. Both use the same `.knot` files. Only the command changes.

## Exploration: fast feedback

Work in the VS Code preview (or with `knot watch`):

- **While you type**, the preview is rebuilt from the cache alone, without
  running any code: prose and Typst changes appear at once, and the chunks you
  edited are outlined in amber until you save.
- **When you save**, Knot runs only what changed: an edited chunk and the chunks
  after it in the same language (the cache key of a chunk includes the key of the
  previous one). Chunks shown in orange are waiting to run; results replace them
  as they are computed.
- **Snapshots** make this cheap. After each chunk, Knot saves the interpreter
  state, so that re-running chunk 12 restores the state after chunk 11 instead of
  running chunks 1 to 11 again.

Errors appear in the preview where they occur: the failing chunk shows the
error, and the following chunks of its language are greyed out until it is
fixed. See [VS Code](./vscode.md) for the preview states.

The exploratory PDF is fast, not certified. A restored snapshot is not always
identical to the state a fresh run would build: some objects cannot be saved
(database connections, Python functions and classes defined in a chunk), and a
file read without being declared with `depends:` is not watched. When a
language's state cannot be restored reliably, turn its snapshots off in the
document header; that chain then always runs from the start:

```yaml
---
snapshots:
  python: false
---
```

See [Cache and execution state](./caching.md) for the details and the
disk-space trade-offs.

## Publication: a clean, complete run

Before publishing, and in continuous integration, build the document with:

```bash
knot build --strict --no-snapshots
```

- `--no-snapshots` runs every chunk again, in document order, in fresh R and
  Python interpreters: no cached result or snapshot replaces an execution.
- `--strict` makes the command fail if the document shows any error. The PDF is
  still written, so that you can read the errors in context.

A successful run means that the whole document executed without errors from a
clean state, and that the PDF shows what that execution produced. It does not
pin package or interpreter versions, freeze input data, fix random seeds or
reproduce network services: record those separately (a lockfile, versioned
data, explicit seeds) when you need them. See the [CLI reference](./cli.md#validating-a-final-document).

## Checks inside the document

Compilation can also verify results. The [Anscombe example](./scientific-example.md)
computes the same statistics in R, in Python and in a mixed variant, exports them
to Typst with `export_data`, and compares them during Typst compilation with
`assert`: the PDF states the check and its tolerance, and a disagreement makes
the compilation fail with a message naming the series and the statistic. See
[Data for Typst](./data-exports.md).
