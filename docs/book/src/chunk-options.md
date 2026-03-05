# Chunk Options Reference

Options are written as YAML comments at the top of a chunk, one per line,
prefixed with `#|`:

~~~typst
```{r}
#| label: my-chunk
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
| `cache` | bool | `true` | If `false`, the chunk always re-executes even if its hash has not changed. |
| `freeze` | list | `[]` | Object names whose xxHash64 fingerprint must not change after this chunk. See [The Freeze Contract](./introduction.md#the-freeze-contract). |

## Display control

| Option | Type | Default | Description |
|---|---|---|---|
| `show` | string | `"both"` | What to display: `"both"`, `"code"`, `"output"`, `"none"`. |
| `echo` | bool | `true` | Alias for `show: "output"` when `false`. Kept for compatibility. |

## Labelling and captions

| Option | Type | Default | Description |
|---|---|---|---|
| `label` | string | — | Chunk identifier. Used as a Typst label (`<label>`) for cross-referencing. |
| `fig-cap` | string | — | Caption for the figure wrapper (enables Typst `#figure`). |

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
| `depends` | list | `[]` | File paths that, when modified, invalidate this chunk's cache. Useful for chunks that read external files. |

Example:

~~~typst
```{r}
#| depends: [data/raw.csv]
data <- read.csv("data/raw.csv")
```
~~~
