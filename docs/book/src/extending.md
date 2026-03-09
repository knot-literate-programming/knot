# Extending Knot

Knot injects a `typst()` function into both R and Python environments, and inlines
`lib.typ` into every assembled `.typ` file. All three are designed to be extended
without modifying Knot itself.

---

## R — adding methods to `typst()`

`typst()` is an S3 generic. Adding support for a new R class is a one-liner:

```r
typst.myclass <- function(obj, ...) {
  # Convert obj to Typst output.
  # Call typst() recursively on components, or use base_plot() for graphics.
}
```

Define this method anywhere in your chunk — no import or registration step required.

### Built-in methods

| Method | Handles |
|---|---|
| `typst.ggplot(obj, ...)` | ggplot2 plots — saved to file via `ggplot2::ggsave()` |
| `typst.data.frame(obj, ...)` | Data frames — exported as CSV and rendered as a table |
| `typst.default(obj, ...)` | Everything else — calls `print(obj)` |

### Example: a custom S3 class

```r
#| echo: true
new_greeting <- function(text) structure(list(text = text), class = "greeting")

typst.greeting <- function(obj, ...) {
  cat("_", obj$text, "_\n", sep = "")
}

typst(new_greeting("Hello from Knot!"))
```

---

## Python — registering handlers with `typst.register()`

`typst()` uses [`functools.singledispatch`](https://docs.python.org/3/library/functools.html#functools.singledispatch).
Register a handler for any type with:

```python
typst.register(MyClass)(lambda obj, **kwargs: ...)
```

`typst` is already in your namespace — no import needed.

### Built-in handlers

| Type | Handles |
|---|---|
| `matplotlib.figure.Figure` | Matplotlib figures — saved as SVG/PNG/PDF |
| `plotnine.ggplot` | plotnine plots — rendered via matplotlib |
| `pandas.DataFrame` | DataFrames — exported as CSV and rendered as a table |

Handlers for optional libraries are registered lazily (on first call to `typst()`),
so missing libraries are silently skipped.

### Example: a custom class

```python
#| echo: true
class Greeting:
    def __init__(self, text):
        self.text = text

typst.register(Greeting)(lambda obj, **kwargs: print(f"_{obj.text}_"))

typst(Greeting("Hello from Knot!"))
```

### Handler signature

Registered functions receive the object as the first positional argument plus any
keyword arguments passed by the caller:

```python
def my_handler(obj, width=None, height=None, **kwargs):
    ...

typst.register(MyClass)(my_handler)
typst(my_obj, width=8, height=5)   # kwargs forwarded to my_handler
```

---

## Typst — overriding `code-chunk` and `knot-state-styles`

`lib.typ` is prepended automatically to every assembled `.typ` file.  Any
definition in your `.knot` file appears *after* it, so Typst's scoping rules
make it shadow the built-in version.

### Overriding `code-chunk`

```typst
#let code-chunk(..args) = {
  // Your custom rendering logic.
  // args.named() contains all named parameters; args.pos() contains positional ones.
}
```

#### Full parameter list

| Parameter | Type | Default | Description |
|---|---|---|---|
| `code` | content / none | `none` | Rendered code block |
| `output` | content / none | `none` | Rendered output block |
| `warnings` | array | `()` | Warning strings |
| `errors` | array | `()` | Error strings |
| `warnings-position` | string | `"below"` | `"below"` or `"inline"` |
| `layout` | string / none | `none` | `"vertical"` or `"horizontal"` (default) |
| `gutter` | length | `0.5em` | Gap between code and output |
| `code-background` | color / none | `none` | Code block background |
| `code-stroke` | stroke / none | `none` | Code block border |
| `code-radius` | length | `0pt` | Code block corner radius |
| `code-inset` | length | `0pt` | Code block padding |
| `output-background` | color | `rgb(255,255,255)` | Output block background |
| `output-stroke` | stroke / none | `none` | Output block border |
| `output-radius` | length | `0pt` | Output block corner radius |
| `output-inset` | length | `0pt` | Output block padding |
| `warning-background` | color | `rgb("#fff4ce")` | Warning block background |
| `warning-stroke` | stroke | `1pt + rgb("#facc15")` | Warning block border |
| `warning-radius` | length | `2pt` | Warning block corner radius |
| `warning-inset` | length | `0.5em` | Warning block padding |
| `width-ratio` | string | `"1:1"` | Column ratio for horizontal layout, e.g. `"2:1"` |
| `align` | alignment / none | `none` | Block alignment |
| `is-inert` | bool | `false` | Live preview: upstream error (white overlay) |
| `is-pending` | bool | `false` | Live preview: execution in progress (orange border) |
| `is-modified` | bool | `false` | Live preview: directly edited chunk (amber border) |
| `is-modified-cascade` | bool | `false` | Live preview: hash-cascaded chunk (muted amber) |
| `state-styles` | dict | `knot-state-styles` | Live preview style overrides |

### Overriding `knot-state-styles`

The live preview visual states (orange border for pending, amber for modified, white
overlay for inert) are controlled by the `knot-state-styles` dictionary.  Override
individual entries to customise the feedback without touching `code-chunk`:

```typst
#let knot-state-styles = (
  ..knot-state-styles,         // keep the other defaults
  pending: (stroke: 3pt + blue),
)
```

> **Note**: `knot-state-styles` affects the live preview only.  It has no effect
> in the final PDF produced by `knot build`.

### Overriding `knot-replace`

`#knot-replace` is called instead of `#code-chunk` when a chunk uses
`show: replace`.  By default it is an alias for `code-chunk` (fallback:
code and output shown one after the other, like `show: both`).

In a [touying](https://typst.app/universe/package/touying) presentation,
override it once after importing the theme so that code appears on overlay 1
and the output **replaces it** at the same position on overlay 2:

```typst
#import "@preview/touying:0.6.3": *
// ... theme setup ...

#let knot-replace(code: none, output: none, ..rest) = alternatives(
  code-chunk(code: code, ..rest),
  code-chunk(output: output, ..rest),
)
```

Then in your `.knot` file:

~~~typst
```{r}
#| show: replace
ggplot(df, aes(x, y)) + geom_line()
```
~~~

`knot-replace` receives the same named parameters as `code-chunk` (see the
full parameter list above), so all styling options (`code-background`,
`output-background`, etc.) pass through to both overlays automatically.
