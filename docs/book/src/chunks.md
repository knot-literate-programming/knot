# Code Chunks

> This chapter covers the mechanics of code chunks. For the complete list of
> options, see [Chunk Options Reference](./chunk-options.md). For output types,
> see the Output chapters.

## Syntax

A code chunk is a fenced Markdown code block whose language tag is wrapped in
braces:

~~~typst
```{r}
x <- 1:10
mean(x)
```
~~~

The braces distinguish executable chunks (`{r}`) from static code blocks (` ```r `),
which Knot passes through as-is to Typst.

Knot executes `r` and `python`. Language tags are case-insensitive and `py` is
an alias of `python`: `{py}`, `{Python}` and `{python}` chunks share the same
Python session. The tag is kept as written when the document is formatted.
Running a chunk in any other language is an error, shown in the PDF and in the
editor; add `#| eval: false` to display such code without running it.

## Controlling what is shown

The `show` option controls what appears in the compiled document:

| Value | Code block shown | Output shown |
|---|---|---|
| `"both"` (default) | Yes | Yes |
| `"code"` | Yes | No |
| `"output"` | No | Yes |
| `"none"` | No | No |

~~~typst
```{r}
#| show: "output"
x <- rnorm(1000)
base_plot(hist(x, col = "steelblue", main = "Distribution of x"))
```
~~~

## Skipping execution

Set `eval: false` to include a chunk in the document without running it:

~~~typst
```{r}
#| eval: false
# This code is shown but not executed
very_slow_function()
```
~~~

## Disabling the cache

By default Knot caches every chunk. Set `cache: false` to force re-execution on
every compile, regardless of whether the code has changed:

~~~typst
```{r}
#| cache: false
# Always re-executes (e.g. for live data, random seeds, timestamps)
Sys.time()
```
~~~

## Labels and cross-references

A chunk label is written in the fence header. A caption is an independent
`caption` option:

~~~typst
```{r fig-histogram}
#| caption: Distribution of simulated data
#| show: "output"
base_plot(hist(rnorm(500), col = "steelblue"))
```

As shown in @fig-histogram, the distribution is approximately normal.
~~~

Either a label or a caption creates a Typst figure of kind `raw`. A label alone
allows cross-references without a displayed caption; a caption alone creates a
figure without a label. A chunk with neither remains an ordinary code/output
block. Typst controls numbering, for example with `#set figure(numbering: "1")`.

To change the default supplement from "Chunk" to "Fragment", configure the
rendering functions before the chunks:

```typst
#let knot-chunk-defaults = (supplement: "Fragment")
#let code-chunk = code-chunk.with(..knot-chunk-defaults)
#let knot-replace = code-chunk
```

The last line applies the same configuration to the default `show: replace`
renderer. If you use a custom presentation renderer, keep that definition and
have it call the configured `code-chunk`. Redefining the defaults dictionary
alone does not change a function that has already captured its original value.

## Execution order and state

Chunks of the same language execute in document order. State accumulates:

~~~typst
```{r}
x <- 42          # x is now defined
```

Some prose in between.

```{r}
x * 2            # outputs 84
```
~~~

If you delete or reorder chunks, the cache is invalidated for everything downstream.
See [The Cache and Invalidation](./introduction.md#the-cache-and-invalidation).
