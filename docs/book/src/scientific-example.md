# Scientific reference: Anscombe's quartet

The [Anscombe project](https://github.com/knot-literate-programming/knot/tree/master/examples/anscombe)
contains three comparable analyses of the same versioned dataset:

- R with ggplot2;
- Python with plotnine;
- independent R and Python calculations, combined graphically in Typst with Gribouille.

The report includes tables, inline values, figures, a bibliography and numerical
checks between languages. It demonstrates [data exports](./data-exports.md) with
`export_data` and explicit dependencies on the input CSV. A separate diagnostic
variant demonstrates warnings, runtime errors, blocked downstream chunks and
recovery.

Follow the example's README to install its dependencies and run `knot build`.
For final verification, use `knot build --no-snapshots` to re-execute the analysis.
A generated PDF may contain runtime diagnostics: inspect the results as well as
the build status.
