# Cache and execution state

Knot caches each document separately under `.knot_cache/v2/`, using its canonical
full path. Two chapters named `main.knot` in different directories do not share
results or interpreter snapshots. Moving a document starts a new cache.

Each language has an independent execution chain. A chunk's identity includes
its language, code, execution options, declared file
dependencies, preceding executed node, and the embedded interpreter scripts.
Changing one of these invalidates that chunk and the following nodes in the same
language. Inline expressions participate in that chain too.

Presentation changes such as `show` reuse raw cached chunk results and render
them again. Figure dimensions, resolution and format affect execution and
therefore invalidate the cache.

## Files and working directory

Interpreters start in the project root (the directory containing `knot.toml`, or
the document's directory without a project configuration). Paths in `depends`
are relative to that root, regardless of the shell directory used to launch Knot.
Dependency contents are hashed; modification times alone are not used.

````typst
```{python}
#| depends: [data/observations.csv]
import pandas as pd
data = pd.read_csv('data/observations.csv')
```
````

Declare the input files that your code reads. Knot does not automatically discover
filesystem, network, environment-variable or database dependencies.

## Reuse, repair and snapshots

Knot verifies the contents of cached result files and snapshots
before reusing them. Missing or damaged files cause re-execution from the affected
node onward. Malformed or incompatible metadata is discarded.

Each compilation starts with fresh interpreters and restores the required cached
prefix. Removed variables cannot leak from a previous compilation. Snapshots are
saved only when enabled for the language, after successful execution; skipped and cached nodes
do not create replacement snapshots. The working directory is restored along
with saved variables.

`eval: false` skips execution and does not advance the interpreter-state chain.
`cache: false` always executes the chunk **and the following nodes in that
language**, since their inputs may have changed even when their code has not.

Python's standard `pickle` cannot faithfully restore every object, including
user-defined functions, classes and open file handles. When such objects are
present, Knot marks the snapshot as non-reusable: on subsequent compilations it
resumes from the last reusable snapshot and re-executes the chunk that created
them and the following chunks of that language. This favors correct execution
over a cache hit.

The first chunk of a chain whose snapshot is not reusable shows a warning, in
the PDF and in the editor, so that the cost is visible. Delete such objects once
they are no longer needed (`del name`), or disable snapshots for the language in
the document header (see below) if the chain replays anyway. The warning does
not fail `knot build --strict`.

A Python snapshot records imported modules by name and imports them again on
restore. Knot also saves the state of the global random generators of `random`
and, when the session imported it, `numpy.random`, so that `random.seed(1)` in
one chunk gives the same numbers in the next whether the chain ran completely
or resumed from a snapshot. Other state kept inside modules (settings, caches,
global generators of other libraries) is not preserved: prefer explicit
generators such as `numpy.random.default_rng(1)` bound to a variable, or disable
Python snapshots when such state matters.

## Document snapshot policy

A YAML header at the beginning of a `.knot` file controls snapshots by language:

```yaml
---
snapshots:
  r: true
  python: false
---
```

Each omitted language defaults to `true`. `false` disables both saving and loading
snapshots for the **entire language chain in this file**. Every compilation
executes its chunks and inline expressions from the beginning in a fresh
interpreter, even for unchanged code or after correcting an error. Cached outputs
remain available for rendering but cannot skip execution. `eval: false` still
skips execution. The editor does not restore runtime snapshots for that language.

Other languages and other files retain their own caching behavior. Re-enabling
snapshots rebuilds missing snapshots before incremental reuse becomes possible.
Existing unused snapshot files can be removed with `knot clean`.

The user is responsible for deciding whether their workspace can be restored.
Successful serialization is not proof that external connections or native resources
will still work in a new process. Disable snapshots if uncertain, or when storing
large environments costs too much disk space. Split independent work into files
to limit replay costs.

The former `freeze` chunk option has been removed. Replace it with this document
setting; no objects are stored separately or checked for mutation.

## Snapshot size warning

By default, Knot warns in the rendered document when snapshots exceed **1 GB
(1,000,000,000 bytes)** cumulatively for one language in one source file. It counts
actual file sizes on disk (including compression and R package context files),
not the size of live objects in memory. Snapshots reused from cache count too;
obsolete snapshots outside the current execution chain do not.

```yaml
---
snapshots:
  r: true
  python: true
snapshot-warning-threshold: 2GB
---
```

The threshold accepts a positive integer byte count or an integer followed by
`B`, `KB`, `MB`, `GB`, `KiB`, `MiB` or `GiB`. Decimal units use powers of 1000;
binary units use powers of 1024. Use `snapshot-warning-threshold: false` to disable
the warning. Each included file has its own threshold; omission means 1 GB.

The first chunk exceeding the threshold displays a Knot warning alongside its
runtime warnings, respecting the usual chunk warning visibility. A crossing in
an inline expression produces a warning at the end of the source document,
leaving the expression itself intact. There is at most one size warning per
language and source file per compilation. It is recalculated on cache reuse and
when the threshold changes, without rerunning otherwise cached computations.

The warning suggests disabling snapshots or splitting independent analyses into
files. It does not delete data or interrupt execution. `knot clean` removes old
unused cache files that are not included in this measurement.

## Full render without snapshots

```sh
knot build --no-snapshots
```

This overrides every file's YAML settings, for every language and included file.
All executable nodes run again in fresh interpreters without saving or restoring
snapshots; skipped nodes remain skipped. The source files are not modified.
Ordinary R/Python warnings remain visible. Snapshot-size warnings disappear
because this run does not produce or reuse snapshots.

Use ordinary `knot build` again to return to the document settings. Missing
snapshots must be rebuilt before incremental reuse. Full re-execution repeats
external effects such as file writes; it does not control changing inputs or
randomness on its own.

## Rebuilding after environment changes

Interpreter and installed-package versions are not yet part of cache identity.
After changing R, Python or their packages, run `knot clean` before rebuilding.
The cache also cannot reconstruct arbitrary external side effects or every form
of process-global state. Use explicit initialization, declared dependencies and
`cache: false` where execution must happen on every build.

Old cache formats are not migrated. They can be removed with `knot clean`.
Copied artifacts in `_knot_files/` are addressed by content hash. An updated
figure gets a different path, so an older preview keeps its own image. Identical
artifacts are reused across builds. Missing files and copy failures are reported.
Unused artifact directories are removed by `knot clean`.

Project builds execute against private cache copies and commit them only during
explicit publication. The LSP admits publications only from its current project
generation. This prevents a superseded save or Run request from overwriting the
current Typst document, figures, snapshots or metadata. A running interpreter call
may finish before cancellation takes effect; external side effects in user code
are not reversible. Independent CLI processes are not coordinated by the LSP.
