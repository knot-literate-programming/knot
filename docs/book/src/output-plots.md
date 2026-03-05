# Plots and Figures

Knot captures plots from R and Python and embeds them in the document as SVG
or PNG images.

## R

Use `typst(current_plot())` after your plotting code to capture the current
graphics device:

~~~typst
```{r}
#| fig-width: 7
#| fig-height: 4
#| show: "output"
plot(mtcars$wt, mtcars$mpg, pch = 19, col = "steelblue",
     xlab = "Weight", ylab = "MPG")
abline(lm(mpg ~ wt, data = mtcars), col = "red")
typst(current_plot())
```
~~~

`typst()` is a helper function loaded into every R session by Knot. It saves the
current plot to a file in the cache and tells Knot to embed it.

### ggplot2

ggplot2 objects must be printed before calling `typst(current_plot())`:

~~~typst
```{r}
library(ggplot2)
p <- ggplot(mtcars, aes(wt, mpg)) +
  geom_point() +
  geom_smooth(method = "lm")
print(p)
typst(current_plot())
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

`typst` and `current_plot` are automatically available in every Python session.

## Figure options

| Option | Default | Description |
|---|---|---|
| `fig-width` | `6` | Width in inches |
| `fig-height` | `4` | Height in inches |
| `fig-dpi` | `150` | Resolution (PNG only) |
| `fig-format` | `"svg"` | `"svg"` or `"png"` |

SVG is recommended for most plots — it scales perfectly at any zoom level and
produces smaller files. Use PNG for plots with many thousands of points where SVG
becomes slow to render.

## Captions and cross-references

~~~typst
```{r}
#| label: fig-scatter
#| fig-cap: Weight vs fuel efficiency in the mtcars dataset.
#| show: "output"
plot(mtcars$wt, mtcars$mpg, pch = 19)
typst(current_plot())
```

@fig-scatter shows a clear negative relationship.
~~~
