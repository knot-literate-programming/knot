# Inline Expressions

An inline expression evaluates a short code snippet and inserts the result
directly into the surrounding text.

## Syntax

```typst
The mean is `{r} mean(x)` and the p-value is `{python} round(p, 4)`.
```

The result is inserted as plain text — no code block, no output label.

## What can be returned

An inline expression should return a **scalar**: a single number, string, or
boolean. Knot formats the result as follows:

- **R**: strips the `[1]` prefix that R normally prepends. `[1] 3.14` becomes `3.14`.
  Quoted strings have their quotes removed. `[1] "Alice"` becomes `Alice`.
- **Python**: inserts whatever `print()` would output for the value.

Short vectors are accepted and rendered verbatim (e.g. `[1] 1 2 3 4 5`), but
complex or multi-line outputs produce an error. Use a code chunk with `show: "output"`
for those.

## Shared state with chunks

Inline expressions share the namespace of their language. An `{r}` expression
sees all variables defined by R chunks above it in the same file:

~~~typst
```{r}
model <- lm(mpg ~ wt, data = mtcars)
coef_wt <- coef(model)["wt"]
```

A one-unit increase in weight is associated with a
`{r} round(coef_wt, 2)` change in fuel efficiency.
~~~

## Caching

Inline expressions are cached with the same chained-hash mechanism as chunks.
If the R or Python state that an expression depends on changes, the expression
re-evaluates automatically.
