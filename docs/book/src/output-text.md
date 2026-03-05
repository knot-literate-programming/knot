# Text Output

When a chunk produces text output (printed values, `cat()`, `print()`, etc.),
Knot captures it and renders it as a code block below the source code.

## R

In R, the last expression in a chunk is automatically printed:

~~~typst
```{r}
summary(mtcars$mpg)
```
~~~

Output:
```
   Min. 1st Qu.  Median    Mean 3rd Qu.    Max.
  10.40   15.43   19.20   20.09   22.80   33.90
```

Use `cat()` or `print()` for explicit output. Multiple print calls produce
multiple output lines.

## Python

In Python, only explicit `print()` calls produce output:

~~~typst
```{python}
import statistics
data = [1, 2, 3, 4, 5]
print(f"Mean: {statistics.mean(data)}")
print(f"Stdev: {statistics.stdev(data):.2f}")
```
~~~

## Suppressing output

Use `show: "code"` to show the code without its output, or `show: "none"` to
silently execute a chunk (useful for setup chunks that only define variables).

~~~typst
```{r}
#| show: "none"
library(ggplot2)
theme_set(theme_minimal())
```
~~~

## Warnings

R and Python warnings are captured separately from standard output. By default
they appear below the output block. Control this with `warning` and `warning-pos`:

~~~typst
```{r}
#| warning: false
log(-1)   ← warning suppressed
```
~~~
