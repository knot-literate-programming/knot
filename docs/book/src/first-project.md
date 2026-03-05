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
└── lib/
    └── knot.typ    ← Typst helpers (imported by main.knot)
```

## Open in VS Code

```bash
code .
```

Open `main.knot`. The Knot extension activates automatically. Click
**Start Preview** in the status bar (or press the Knot button in the editor
toolbar) to open the live PDF preview.

## Write your first document

Replace the contents of `main.knot` with:

~~~typst
#import "lib/knot.typ": *
#show: knot-init

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

Save the file. The preview updates within a second.

## Compile to PDF

```bash
knot build      # writes my-report.pdf
```

Or use watch mode, which rebuilds automatically on every save:

```bash
knot watch      # rebuilds on save + opens typst watch for PDF
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
