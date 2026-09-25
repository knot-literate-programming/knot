# Chunk Options Reference

Options are written as YAML comments at the top of a chunk, one per line,
prefixed with `#|`:

~~~typst
```{r my-chunk}
#| echo: false
#| fig-width: 6
plot(1:10)
typst(current_plot())
```
~~~

Options can also be set globally in `knot.toml` under `[chunk-defaults]`,
`[r-chunks]`, or `[python-chunks]`. Per-chunk options always override defaults.

---

## Execution control

| Option | Type | Default | Description |
|---|---|---|---|
| `eval` | bool | `true` | If `false`, the chunk is not executed and produces no output. |
| `cache` | bool | `true` | If `false`, the chunk and following nodes in its language re-execute on each compilation. |

## Display control

| Option | Type | Default | Description |
|---|---|---|---|
| `show` | string | `"both"` | What to display: `"both"`, `"code"`, `"output"`, `"none"`, `"replace"`. |
| `echo` | bool | `true` | Alias for `show: "output"` when `false`. Kept for compatibility. |

The `"replace"` value is designed for **presentation slides**: code appears first,
then the output replaces it at the same position on the next overlay click.
It requires a compatible slide framework such as
[touying](https://typst.app/universe/package/touying) — see
[Overriding `knot-replace`](./extending.md#overriding-knot-replace) for the
one-line setup.

## Labelling and captions

| Option | Type | Default | Description |
|---|---|---|---|
| `caption` | string | — | Typst content displayed as a figure caption; independent of the label. |

Write the label in the fence header, e.g. ` ```{r my-chunk} `, and refer to it
with `@my-chunk`. It is not a `#| label:` option. A label or a caption causes
the chunk to be wrapped as a Typst figure; neither is required for the other.
See [Labels and cross-references](./chunks.md#labels-and-cross-references).

## Figure sizing

| Option | Type | Default | Description |
|---|---|---|---|
| `fig-width` | number | `6` | Figure width in inches. |
| `fig-height` | number | `4` | Figure height in inches. |
| `fig-dpi` | number | `150` | Resolution in dots per inch (raster formats). |
| `fig-format` | string | `"svg"` | Output format: `"svg"` or `"png"`. |

## Warnings

| Option | Type | Default | Description |
|---|---|---|---|
| `warning` | bool | `true` | Whether to capture and display R/Python warnings. |
| `warning-pos` | string | `"below"` | Where to show warnings: `"above"` or `"below"` the output. |

## Layout

| Option | Type | Default | Description |
|---|---|---|---|
| `layout` | string | `"vertical"` | How to arrange code and output: `"vertical"` or `"horizontal"`. |
| `align` | string | `"left"` | Alignment of the chunk's content (code, output, warnings, errors): `"left"`, `"center"` or `"right"`. A chunk with a `label` or `caption` is aligned the same way; only its caption is centred. |

## Code styling (codly)

Options prefixed with `codly-` are passed directly to the
[codly](https://typst.app/universe/package/codly) Typst package for syntax
highlighting customisation:

~~~typst
```{r}
#| codly-stroke: 2pt + red
#| codly-lang-radius: 8pt
x <- 1
```
~~~

Refer to the codly documentation for the full list of available options.

## Dependencies

| Option | Type | Default | Description |
|---|---|---|---|
| `depends` | list | `[]` | File paths relative to the project root that, when modified, invalidate this chunk's cache. Useful for chunks that read external files. |

Example:

~~~typst
```{r}
#| depends: [data/raw.csv]
data <- read.csv("data/raw.csv")
```
~~~
