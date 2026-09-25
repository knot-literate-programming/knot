# Your First Project

## Create a project

```bash
knot init my-report
cd my-report
```

This creates:

```
my-report/
├── knot.toml       ← project configuration
├── main.knot       ← your document
└── .gitignore      ← ignores the generated files
```

No Typst import is needed for Knot itself: its Typst library is embedded in
every compiled `.typ` file.

## Open in VS Code

```bash
code .
```

Open `main.knot`. The Knot extension activates automatically. Click
**Open Preview** in the editor title bar (or run **Knot: Open Preview** from the
command palette) to open the live preview.

## Write your first document

The template starts with a codly configuration (syntax highlighting), which the
styles in `knot.toml` rely on: keep it. Replace the rest of `main.knot` with:

~~~typst
= My First Report

```{r}
x <- c(2, 4, 6, 8, 10)
summary(x)
```

The mean of `x` is `{r} mean(x)` and its standard deviation is `{r} round(sd(x), 2)`.

```{python}
import math
values = [1, 4, 9, 16, 25]
print(f"Sum of squares: {sum(values)}")
```
~~~

Save the file. The preview shows each chunk's result as soon as it has run.

## Compile to PDF

```bash
knot build      # writes main.pdf
```

Or use watch mode, which rebuilds automatically on every save:

```bash
knot watch      # rebuilds main.pdf on every save
```

## What just happened

When you saved `main.knot`, Knot ran a three-pass pipeline:

1. **Plan** — parsed the document, computed a SHA-256 hash for each chunk,
   and decided which chunks needed to execute (all of them, since this is
   the first run).

2. **Execute** — ran the R chunks sequentially in an R subprocess, and the
   Python chunk in a Python subprocess. The two languages ran in parallel.
   Results were written to the cache.

3. **Assemble** — interleaved the chunk outputs with the surrounding Typst
   source and wrote `main.typ`.

On the next save, if you only change the prose, none of the chunks re-execute —
their cached outputs are reused instantly.

## Next steps

- [Document Structure](./document-structure.md) — learn the full `.knot` format.
- [Chunk Options](./chunk-options.md) — control what is shown, how figures are sized, and more.
- [VS Code](./vscode.md) — preview, sync, formatting, and diagnostics.
