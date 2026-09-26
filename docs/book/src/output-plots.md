# Plots and Figures

Knot captures plots from R and Python and embeds them in the document as SVG
or PNG images.

## R

### Base graphics

Wrap base graphics code in `base_plot({ ... })`. The code runs on a fresh
graphics device, and the resulting figure is embedded in the document:

~~~typst
```{r}
#| fig-width: 7
#| fig-height: 4
#| show: output
base_plot({
  plot(mtcars$wt, mtcars$mpg, pch = 19, col = "steelblue",
       xlab = "Weight", ylab = "MPG")
  abline(lm(mpg ~ wt, data = mtcars), col = "red")
})
```
~~~

`base_plot()` is loaded into every R session by Knot. It also accepts `width`,
`height`, `dpi` and `format` arguments, which override the chunk options.

### ggplot2

Use `typst(p)` to embed the ggplot object `p`:

~~~typst
```{r}
library(ggplot2)
p <- ggplot(mtcars, aes(wt, mpg)) +
  geom_point() +
  geom_smooth(method = "lm")
typst(p)
```
~~~

## Python (Matplotlib)

~~~typst
```{python}
import matplotlib.pyplot as plt
import numpy as np

x = np.linspace(0, 2 * np.pi, 100)
plt.plot(x, np.sin(x))
plt.title("Sine wave")
typst(current_plot())
```
~~~

`typst` and `current_plot` are available in every Python session, with no
import. `typst(fig)` also accepts a Matplotlib figure or a plotnine plot.

## Figure options

| Option | Default | Description |
|---|---|---|
| `fig-width` | `7` | Width in inches |
| `fig-height` | `5` | Height in inches |
| `dpi` | `300` | Resolution (PNG only) |
| `fig-format` | `"svg"` | `"svg"` or `"png"` |

SVG is recommended for most plots: it scales perfectly at any zoom level and
produces smaller files. Use PNG for plots with many thousands of points, where
SVG becomes slow to render.

## Captions and cross-references

The label goes in the fence header, the caption in the `caption` option:

~~~typst
```{r fig-scatter}
#| caption: Weight vs fuel efficiency in the mtcars dataset.
#| show: output
base_plot(plot(mtcars$wt, mtcars$mpg, pch = 19))
```

@fig-scatter shows a clear negative relationship.
~~~

See [Labels and cross-references](./chunks.md#labels-and-cross-references).
