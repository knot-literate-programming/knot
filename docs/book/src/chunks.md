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
hist(x, col = "steelblue", main = "Distribution of x")
typst(current_plot())
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

A labeled chunk becomes a referenceable Typst figure when it also has a caption:

~~~typst
```{r}
#| label: fig-histogram
#| fig-cap: Distribution of simulated data
#| show: "output"
hist(rnorm(500), col = "steelblue")
typst(current_plot())
```

As shown in @fig-histogram, the distribution is approximately normal.
~~~

Without a caption, the label is still emitted as a Typst label but no `#figure`
wrapper is added.

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
