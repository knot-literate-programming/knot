# Exporting data to Typst

Use `export_data(data, name)` in an R or Python chunk to make a JSON dataset
available to Typst. This is useful for drawing figures with a Typst package
such as Gribouille or Lilaq, while keeping the calculations in R or Python.
The helper is loaded automatically, like `typst()`.

~~~typst
```{r}
#| show: none
results <- data.frame(x = c(1, 2), y = c(3, 4))
export_data(results, "r-results")
```

```{python}
#| show: none
results = [{"x": 1, "y": 3}, {"x": 2, "y": 4}]
export_data(results, "python-results")
```

#let r-results = knot-data.at("r-results")
#let python-results = knot-data.at("python-results")
#assert.eq(r-results, python-results)

The second ordinate is #r-results.at(1).y.
~~~

`knot-data` is a Typst dictionary populated in document order. Read an export
**after its producing chunk** (or after the included chapter containing it).
Names must be non-empty strings and unique across the assembled document;
a duplicate causes an explicit Typst error rather than replacing data.
Multiple exports per chunk are supported, alongside ordinary text, tables or
figures. `show: none` hides the chunk but keeps its exports available.

## Data representation

- R uses `jsonlite`: data frames become arrays of row dictionaries, named lists
  become dictionaries, and atomic length-one values become scalars. Use
  `I(...)` to preserve an atomic length-one array. `NULL` and missing values
  become JSON `null`; non-finite numeric values also follow jsonlite's `na = "null"`
  conversion. Numeric values are written with `digits = NA`.
- Python accepts JSON-compatible dictionaries, lists, strings, numbers,
  booleans and `None`. For pandas use
  `export_data(df.to_dict(orient="records"), "results")`; convert unsupported
  values such as timestamps explicitly. NaN and infinity are rejected: replace
  missing values with `None` when appropriate.
- Typst reads the JSON using `json()`: arrays remain arrays, objects become
  dictionaries, and `null` becomes `none`. This transfers data, not arbitrary
  R/Python objects, classes or shared references.

## Execution and cache

Knot stores the JSON in the chunk cache with a checksum and publishes a copy
under `_knot_files/` alongside the generated document. User code does not need
to know either path. Missing or modified cached exports invalidate the result;
missing published copies are recreated from a valid cache. Exports from a
chunk that fails are not exposed to Typst, even if it exported before failing.

R and Python may export independently in parallel. They should read common
input files, declared with `depends` as usual; exporting does **not** establish
an execution dependency between languages. Within one language and one
`.knot` file, subsequent chunks already share variables and need no export.

Exports also work with snapshots disabled, including `knot build --no-snapshots`.
They follow the same publication rules as figures: an obsolete compilation
cannot publish its exports through Knot's managed project build.

During live preview, pending, failed or skipped chunks may not have exported
their data yet. Guard figures that need those data if you want the rest of the
preview to remain renderable:

```typst
#if "r-results" in knot-data {
  let values = knot-data.at("r-results")
  // Draw the figure using values.
} else {
  [Results pending or unavailable.]
}
```

The guard displays absence; it does not reuse results from a previous execution.
