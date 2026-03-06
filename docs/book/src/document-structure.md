# Document Structure

A `.knot` file is a valid Typst document with two additions: **code chunks** and
**inline expressions**. Everything else — headings, paragraphs, equations, figures,
references — is standard Typst.

## The required import

Every `.knot` document must start with:

```typst
#import "lib/knot.typ": *
#show: knot-init
```

`knot-init` installs the Typst functions (`code-chunk`, etc.) that Knot's output
relies on. Without it, the compiled `.typ` will not render correctly.

## Code chunks

A code chunk is a fenced block with a language tag in braces:

~~~typst
```{r}
x <- 1:10
mean(x)
```
~~~

~~~typst
```{python}
import numpy as np
np.mean([1, 2, 3])
```
~~~

The language tag (`r`, `python`) determines which interpreter runs the block.

Options are specified as YAML comments at the top of the block, prefixed with `#|`:

~~~typst
```{r}
#| label: summary-stats
#| echo: false
#| fig-width: 6
x <- rnorm(100)
hist(x)
typst(current_plot())
```
~~~

See [Chunk Options](./chunk-options.md) for the complete reference.

## Inline expressions

An inline expression evaluates a short snippet and inserts the result into the
surrounding prose:

```typst
The average is `{r} mean(x)` and the count is `{python} len(values)`.
```

Inline expressions share the same namespace as code chunks — `x` defined in an
R chunk above is available in a later `{r}` inline expression.

Inline expressions should return a simple scalar (a number, a string, a boolean).
For complex objects, use a code chunk instead.

## Multi-file projects

Large projects can split content across multiple `.knot` files. Declare them in
`knot.toml`:

```toml
[document]
main = "main.knot"
includes = ["chapter1.knot", "chapter2.knot"]
```

Each file gets its own isolated R and Python environment. Variables defined in
`chapter1.knot` are **not** visible in `chapter2.knot`. If you need to share data
between files, write it to disk (e.g., an RDS file, a CSV, a pickle) in one file
and read it in the next.

The main file can contain a `/* KNOT-INJECT-CHAPTERS */` placeholder where the
compiled include files will be inserted:

```typst
#import "lib/knot.typ": *
#show: knot-init

= My Book

/* KNOT-INJECT-CHAPTERS */

= Appendices
```

If the placeholder is missing, Knot will automatically append the includes at the
very end of the `main.knot` file.
