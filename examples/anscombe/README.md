# Anscombe: a scientific reference project

One question, one local dataset, four comparable `.knot` variants. The normal
build combines the three successful analyses into a five-page report; the
intentional failures have a separate entry point.

| File | Calculation | Graphics |
| --- | --- | --- |
| `anscombe-r.knot` | All four series in R | ggplot2 |
| `anscombe-python.knot` | All four series in Python | plotnine |
| `anscombe-mixed.knot` | Series I–II in R, III–IV in Python | Gribouille 0.7.0 |
| `anscombe-errors.knot` | The same summaries, with deliberate failures | Diagnostics and available results |

The report demonstrates included files, inline values, external dependencies,
hidden computation, figures, a bibliography and named JSON exports to Typst.
The three successful variants use the same scales and precomputed regression
lines. Their numerical results are checked against each other during Typst
compilation, with absolute tolerance `1e-8`. Missing exports are displayed as
unavailable so that the document also works during progressive rendering.

## Install and build

Install Knot, R, Python 3.10 or newer, and Typst 0.15 or newer. From this directory:

```sh
Rscript -e 'install.packages(c("ggplot2", "svglite", "jsonlite", "digest"), repos="https://cloud.r-project.org")'
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
knot build
```

On Windows, activate the environment with `.venv\Scripts\Activate.ps1` in
PowerShell instead. Knot runs `python3` from `PATH` unless `knot.toml` selects an
interpreter. To use the virtual environment without activating it (in the editor
too), add to `knot.toml`:

```toml
[tools]
python = '.venv/bin/python'          # Windows: '.venv\Scripts\python.exe'
```

See [External tools](../../docs/book/src/configuration.md#external-tools).

Open `main.pdf`. Typst downloads its pinned packages on the first compilation;
subsequent builds can use the local package cache. The CSV is versioned, so the
analysis itself does not download data. Python dependency ranges are bounded,
not a complete environment lock; record the installed versions for archival use.

For a full re-execution without snapshots:

```sh
knot build --no-snapshots
```

The report uses separate interpreter state for each included file. In the mixed
variant, the two languages can execute independently: neither reads an artifact
produced by the other. `export_data` makes their results available to Typst via
`knot-data`; Gribouille draws the exported points and lines without fitting a
second model.

## Deliberate errors and recovery

Make a disposable copy of this example and replace its `knot.toml` with
`knot-errors.toml`. Run `knot build` there and open `errors-main.pdf`.

1. An R warning is visible, and execution continues.
2. `DELIBERATE_R_ERROR` stops the R chain; its downstream summary is unavailable.
3. The independent Python chain still computes its summary.
4. `DELIBERATE_PYTHON_ERROR` prevents export of the line coordinates.
5. Remove the two marked failing statements and rebuild. Both summaries and the
   Python line coordinates become available. Compare with `--no-snapshots`.
6. Reintroduce the errors: the old downstream results must become unavailable.

A PDF containing runtime diagnostics can still be generated successfully: the
existence of a PDF alone does **not** certify that all scientific calculations
succeeded. `knot build` lists the errors on stderr; to make them fail the
command, validate the document with a full re-execution:

```sh
knot build --strict --no-snapshots
```

This fails for the errors variant (its diagnostic PDF is still written) and
succeeds for the reference report.

## Validation

The CLI PDF integration suite copies this fixture to temporary projects and
checks numerical results, cache reuse after a prose edit, invalidation after a
CSV edit, full replay, independent language progress, repair, and recurrence of
errors without stale exports. Run it from the repository root with the runtime
dependencies above available on `PATH`:

```sh
cargo test -p knot-cli --locked --test integration_pdf anscombe -- --ignored
```

These tests run in the existing Linux/macOS/Windows PDF integration CI job.
They check exported numerical values and PDF compilation, not pixel identity
between graphics libraries or platforms.

For an interactive VS Code check, open this directory as a project, preview
`main.knot`, navigate between the source and preview, edit prose, then edit a
CSV value and observe the updated calculations. Repeat with the error project
and correct each failure. This editor check is manual, separate from CLI tests.

## Data and attribution

`data/anscombe.csv` is a long-form transcription of R's
[`datasets::anscombe`](https://stat.ethz.ch/R-manual/R-patched/library/datasets/html/anscombe.html):
44 observations, eleven per series, with columns `series`, `x`, `y`. No values
were generated or rounded again. The source values have finite precision;
statistics across the four series are approximately, rather than exactly, equal.

The scientific reference is F. J. Anscombe, *Graphs in Statistical Analysis*,
The American Statistician 27(1), 17–21 (1973), cited in `references.bib`.
The report reproduces the statistical demonstration, not the article's layout
or prose. The repeated short calculations are deliberate: each variant remains
readable without following a separate helper module.
