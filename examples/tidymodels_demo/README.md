# Tidymodels Demo Example

This example demonstrates how to use knot for writing a machine learning book with R tidymodels.

## Features Demonstrated

### 1. Inline Expressions

**Display values:**
```typst
The dataset has `{r} nrow(data)` rows.
```

**Side-effects only (`output=false`):**
```typst
`{r, output=false}
  model_name <- "Logistic Regression"
  n_predictors <- 4
`
```

Using `output=false` executes code for its side-effects without producing output in the document.

### 2. Rich Output

**DataFrames as tables:**
```r
typst(conf_mat_data$table)  # Confusion matrix as Typst table
```

**Plots:**
```r
gg <- ggplot(...) + geom_point()
typst(gg, width = 8, height = 5)
```

### 3. Caching

```r
#| cache: true
```

Expensive computations (model fitting, data splitting) are cached and only re-executed when code changes.

### 4. Chunk Options

- `echo: false` - Hide code, show output only
- `eval: true/false` - Control execution
- `cache: true/false` - Enable caching
- `caption: "..."` - Add caption to chunk

## Compiling

```bash
cd examples/tidymodels_demo
knot compile chapter_demo.knot
```

## Prerequisites

Install required R packages:

```r
install.packages(c("tidyverse", "tidymodels", "palmerpenguins"))

# Install knot R package
cd knot-r-package && R CMD INSTALL .
```

## Why This Workflow?

1. **Reproducibility:** All code is executed in order, cached intelligently
2. **Live values:** Inline expressions show computed values directly in text
3. **Publication quality:** Typst output for professional documents
4. **Fast iteration:** Caching makes recompilation instant
5. **Clear intent:** `output=false` makes side-effects explicit
