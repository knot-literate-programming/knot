# Chunk Options Reference

Options are written as YAML comments at the top of a chunk, one per line,
prefixed with `#|`:

~~~typst
```{r my-chunk}
#| show: output
#| fig-width: 6
library(ggplot2)
typst(ggplot(mtcars, aes(wt, mpg)) + geom_point())
```
~~~

Options can also be set for the whole project in `knot.toml`, under
`[chunk-defaults]` (all languages) and `[r-chunks]` or `[python-chunks]` (one
language); per-chunk options override them. See
[Configuration](./configuration.md). Unknown options produce a warning and are
ignored; invalid values are errors, and the chunk is then not executed.

## All options

<!-- BEGIN CHUNK OPTIONS (generated: cargo run -p knot-core --example chunk_options_reference) -->
| Option | Type | Default | In `knot.toml` | Description |
|---|---|---|---|---|
| `eval` | bool | `true` | yes | Whether to run the chunk; with false, its code is only displayed. |
| `show` | string | `"both"` | yes | What to display: both, code, output, none (nothing, but diagnostics) or replace (slides). |
| `cache` | bool | `true` | yes | Whether to reuse cached results; with false, the chunk and the following ones in its language re-execute. |
| `caption` | string | — | no | Figure caption (Typst content); a caption or a label makes the chunk a figure. |
| `depends` | list of paths | `[]` | yes | Files, relative to the project root, whose changes re-execute the chunk. |
| `fig-width` | number | `7` | yes | Figure width in inches. |
| `fig-height` | number | `5` | yes | Figure height in inches. |
| `dpi` | integer | `300` | yes | Resolution of raster (png) figures, in dots per inch. |
| `fig-format` | string | `"svg"` | yes | Figure file format: svg or png. |
| `layout` | string | `"horizontal"` | yes | Arrangement of code and output when both are shown: horizontal or vertical. |
| `warnings-visibility` | string | `"below"` | yes | Where R/Python warnings appear: below the chunk, inline next to the output, or none. |
| `gutter` | string | — | yes | Space between code and output blocks (Typst length). |
| `code-background` | string | — | yes | Background color for code container (Typst color). |
| `code-stroke` | string | — | yes | Border stroke for code container (Typst stroke). |
| `code-radius` | string | — | yes | Corner radius for code container (Typst length). |
| `code-inset` | string | — | yes | Internal padding for code container (Typst length). |
| `output-background` | string | — | yes | Background color for output container (Typst color). |
| `output-stroke` | string | — | yes | Border stroke for output container (Typst stroke). |
| `output-radius` | string | — | yes | Corner radius for output container (Typst length). |
| `output-inset` | string | — | yes | Internal padding for output container (Typst length). |
| `warning-background` | string | — | yes | Background color for warning container (Typst color). |
| `warning-stroke` | string | — | yes | Border stroke for warning container (Typst stroke). |
| `warning-radius` | string | — | yes | Corner radius for warning container (Typst length). |
| `warning-inset` | string | — | yes | Internal padding for warning container (Typst length). |
| `width-ratio` | string | — | yes | Width ratio for horizontal layout (e.g., "1:1", "2:1"). |
| `align` | string | — | yes | Alignment of the chunk's content: left, center or right (default: left, also in figures). |
<!-- END CHUNK OPTIONS -->

Values written as Typst colors, strokes or lengths (for example
`code-background: rgb("#f5f5f5")` or `code-stroke: 1pt + gray`) are passed to
Typst as written. `code-border`, `code-padding`, `output-border`,
`output-padding`, `warning-border` and `warning-padding` are accepted as
aliases of the corresponding `-stroke` and `-inset` options.

## Display: `show`

- `both` shows the code and its output, arranged by `layout`.
- `code` or `output` shows only one of them.
- `none` hides the chunk; diagnostics about it (errors, ignored options) are
  still shown, and its data exports remain available.
- `replace` is designed for **presentation slides**: code appears first, then
  the output replaces it at the same position on the next overlay click. It
  requires a compatible slide framework such as
  [touying](https://typst.app/universe/package/touying) — see
  [Overriding `knot-replace`](./extending.md#overriding-knot-replace) for the
  one-line setup.

## Labels and captions

Write the label in the fence header, e.g. ` ```{r my-chunk} `, and refer to it
with `@my-chunk`. It is not a `#| label:` option. A label or a caption makes the
chunk a Typst figure; neither is required for the other. The content of a
figure is aligned like any other chunk (see `align`); only the caption is
centred. See [Labels and cross-references](./chunks.md#labels-and-cross-references).

## Code styling (codly)

Options prefixed with `codly-` are passed directly to the
[codly](https://typst.app/universe/package/codly) Typst package for syntax
highlighting customisation. They require codly to be imported and initialised
in the document (the `knot init` template does it). Without such an import, they
are ignored and a warning, at the end of the PDF and in the editor, names them:

~~~typst
#import "@preview/codly:1.3.0": *
#show: codly-init

```{r}
#| codly-stroke: 2pt + red
#| codly-lang-radius: 8pt
x <- 1
```
~~~

Refer to the codly documentation for the full list of available options.

## Dependencies

`depends` lists files, relative to the project root, that the chunk reads. When
one of them changes, the chunk and the following chunks in its language are
executed again, and `knot watch` rebuilds the document.

A file that does not exist is an error, shown on the chunk in the PDF and in the
editor: the chunk is not executed and the following chunks in its language are
suspended, until the file is created.

~~~typst
```{r}
#| depends: [data/raw.csv]
data <- read.csv("data/raw.csv")
```
~~~
