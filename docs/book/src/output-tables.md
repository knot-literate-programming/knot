# DataFrames and Tables

Knot automatically converts R data frames and Python pandas DataFrames into
Typst tables.

## R

Pass a data frame to `typst()`:

~~~typst
```{r}
#| show: "output"
typst(head(mtcars, 5))
```
~~~

This produces a formatted Typst table with column headers. The table respects
Typst's standard table styling.

## Python

~~~typst
```{python}
import pandas as pd

df = pd.DataFrame({
    "name":  ["Alice", "Bob", "Carol"],
    "score": [92, 85, 97],
    "grade": ["A", "B", "A+"]
})
typst(df)
```
~~~

## Combining a table and a plot

A chunk can emit both a table and a plot. Use `typst()` twice:

~~~typst
```{r}
#| show: "output"
df <- aggregate(mpg ~ cyl, data = mtcars, mean)
typst(df)
barplot(df$mpg, names.arg = df$cyl, ylab = "Mean MPG", xlab = "Cylinders")
typst(current_plot())
```
~~~
