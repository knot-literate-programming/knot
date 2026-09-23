# Cache and execution state

Knot caches each document separately under `.knot_cache/v2/`, using its canonical
full path. Two chapters named `main.knot` in different directories do not share
results or interpreter snapshots. Moving a document starts a new cache.

Each language has an independent execution chain. A chunk's identity includes
its language, code, execution options, `freeze` declarations, declared file
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

Knot verifies the contents of cached result files, snapshots and frozen objects
before reusing them. Missing or damaged files cause re-execution from the affected
node onward. Malformed or incompatible metadata is discarded.

Each compilation starts with fresh interpreters and restores the required cached
prefix. Removed variables cannot leak from a previous compilation. Snapshots are
saved only for chains without active freeze declarations, after successful execution; skipped and cached nodes
do not create replacement snapshots. The working directory is restored along
with saved variables.

`eval: false` skips execution and does not advance the interpreter-state chain.
`cache: false` always executes the chunk **and the following nodes in that
language**, since their inputs may have changed even when their code has not.

Python's standard `pickle` cannot faithfully restore every object, including
user-defined functions, classes and open file handles. When such objects are
present, Knot marks the snapshot as non-reusable and replays the affected prefix
on subsequent compilations. This favors correct execution over a cache hit.

`freeze: [x, y]` disables snapshot creation and restoration for the **entire
language chain of that document**, including nodes before the declaration.
Every compilation executes this chain from the beginning in a fresh interpreter,
even when the source is unchanged or only one erroneous chunk was corrected.
The editor also refuses to restore snapshots from a frozen chain for inspection.
Cached outputs can be stored for rendering, but do not skip execution in that chain.

The named objects remain live throughout execution. Their fingerprints are
checked before successful results are cached; a mutation makes following nodes
inert. This avoids reconstructing objects from separate snapshots, which could
silently break shared references. It does not make external dependencies or
nondeterministic code reproducible automatically.

Declarations in `eval: false` chunks do not activate this policy. Other language
chains and other `.knot` files keep their normal cache. Split independent analyses
into separate files to limit the lifetime of large variables and the cost of replay.

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
