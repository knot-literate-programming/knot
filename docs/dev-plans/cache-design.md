# Cache and execution state: design

Status: proposal for discussion (September 2026). Related: #85, #88, #124, #125,
#167, #169, #170, #76.

The cache is one of Knot's promises: *your code runs in document order while you
keep writing*. Linear execution is only bearable if Knot re-runs little, and only
trustworthy if what it skips is exactly what a complete run would produce. This
document states the guarantees we want, where the current design falls short,
one concept that ties the fixes together (the **captured state**), and a
sequence of changes.

## 1. Promise and invariants

1. **Equivalence.** For the same sources, declared inputs and interpreters, an
   incremental compilation produces the same document as a complete run
   (`knot build --no-snapshots`). This is the reference, and CI can check it.
2. **Visible degradation.** When Knot cannot guarantee equivalence, it replays
   (correct but slower) and says so, in the PDF and the editor. It never
   silently diverges (the rule of #96, applied to the cache).
3. **Proportional work.** The cost of a save is proportional to what the change
   affects, not to the length of the document.
4. **Bounded resources.** Disk and time spent on caching stay within a small
   multiple of what the computation itself needs.

Today Knot keeps (1) except for the known gaps below, keeps (2) for unpicklable
objects only, meets (3) poorly, and (4) only thanks to a warning at 1 GB.

## 2. How it works today

- **Keys.** A chunk's key is `H(code, options, previous key in its language,
  declared files, interpreter path, helper scripts version)`. Each language of
  each file is one linear chain.
- **Snapshots.** After each executed chunk, the whole workspace of the language
  is serialized (Python: one `pickle.dump` of `__main__`, imported modules by
  name and submodules, the `random`/`numpy.random` states, the working
  directory; R: `save.image` plus the attached packages). The files are hashed.
- **Reuse.** A chunk's result is reused only if its snapshot is valid: a later
  miss may need to resume from it. Resuming restores the snapshot of the
  preceding chunk into a fresh interpreter.
- **Fallback.** A snapshot that cannot be reused (unpicklable objects) makes the
  chain replay from the last reusable one, with a warning (#116).
- **Isolation.** Each compilation works in a private copy of the caches and
  publishes atomically (`ProjectBuild`); the preview reads them in place.

## 3. Where it falls short

| Problem | Invariant | Evidence |
|---|---|---|
| Any edit re-runs every later chunk of the language, even independent ones (a plot tweak re-fits the models below it) | 3 | design of the chain |
| A full snapshot after every chunk: N chunks × workspace size, written and hashed at each save | 4, 3 | 1 GB warning (#47); `bench_live` |
| Restored state misses parts of the real state: module-internal state (library settings, other RNGs such as `torch.manual_seed`), environment variables, R `options()`/`par()`, memory shared below the language's references (NumPy views, data.table aliases), submodules (fixed in #167), R external pointers | 1, 2 | #167, #169, `caching.md` |
| Inputs Knot does not see: files read without `depends:`, files written by one chunk (or one language) and read by another, package upgrades at the same interpreter path | 1 | `caching.md` ("run `knot clean`") |
| Each save copies the committed caches into a workspace | 3, 4 | #125 |
| Object-level freezing split the reference graph and could not be verified | 1 | `freeze`, removed in #46 |

## 4. The central concept: the captured state

Every mechanism above — snapshots, reuse, and any smarter invalidation — relies
on one assumption: **the state Knot captures is all the state that influences
later chunks.** Snapshots restore the captured state; a replay recomputes the
real state. They agree exactly when the captured state is complete.

So instead of patching mechanisms one by one, make the **state model explicit**:

- **Captured state** = the workspace graph (serialized in one pass, so
  references survive) + a registry of **process-state components**, each with
  `get()` (a serializable value) and `set(value)`: RNG states, working
  directory, environment variables, `sys.modules` names, `sys.path`, matplotlib
  `rcParams`, pandas options, warnings filters; in R, `options()`, `par()`, the
  search path, `Sys.getenv()`.
- **Known-incomplete state** is detected, not ignored: memory shared below the
  references (#169), unpicklable objects, R external pointers. Detection makes
  the snapshot non-reusable, so invariant 2 holds.
- **Unknown state** (a library's private cache, a global registry) remains. It
  is the residual risk of any cache of interpreter state, and the reason why a
  complete run stays the reference, and why verification (5.2) matters.

Every direction below is either a way to make the captured state more complete,
or a way to exploit it better. The same model serves snapshots, fingerprints
(5.3), deduplication (5.4) and checkpoints (5.5).

## 5. Directions

### 5.1 Complete the state model (correctness)

- The process-state registry above, with built-in components for R, Python,
  NumPy, pandas and matplotlib. Components are saved with the snapshot and
  restored before the workspace. A library that is not loaded contributes
  nothing and costs nothing.
- **Deterministic interpreters**: start Python with a fixed `PYTHONHASHSEED`, so
  that set and dict-of-str iteration orders, and serialized bytes, are stable
  between runs (a prerequisite for fingerprints, and a reproducibility gain).
- **Environment identity**: record the versions of the packages actually loaded
  (`importlib.metadata`, `packageVersion`) with each snapshot. A different
  version on the next run invalidates the chain from the first chunk that loaded
  that package, instead of requiring `knot clean`. This is also the missing
  piece of reproducible builds (#88).
- **Detection** of incomplete state (#169), including R external pointers (an R
  replay marker is needed).

*Obstacles.* Components must be cheap to read (they run after every chunk), and
their `set` must not have side effects when the value is unchanged. Unknown
libraries: provide a public hook (`knot.state.register(get, set)` in Python, the
same in R) rather than an ever-growing built-in list.

### 5.2 Verification: make divergence observable (trust)

- **`knot build --audit`**: run the document incrementally and completely and
  report every chunk whose outputs differ, with the reason when known
  (non-determinism, unseeded random, time, missing state component). This turns
  invariant 1 into something users can check, and it catches unknown state.
- **Differential testing in CI**: on the examples, apply random edit sequences
  (edit, insert, delete, reorder chunks; change a data file) and assert that
  each incremental result equals a fresh complete run. This is the oracle that
  keeps every optimization below honest.

*Obstacles.* Legitimately non-deterministic chunks (timestamps, unseeded random)
must be reported, not fail the suite: the audit classifies them, and the
differential tests use deterministic documents.

### 5.3 State fingerprints and early cutoff (proportional work)

The chain re-runs everything after an edit because a chunk's key includes the
*key* of the previous chunk, that is, its code history. Replace it with the
*state* it leaves:

`key(k) = H(code_k, options_k, fingerprint(state after k−1), declared files)`

After re-running an edited chunk, Knot fingerprints the captured state it
leaves. If the fingerprint equals the previous run's, every later chunk has the
same inputs: their results and snapshots are reused (**early cutoff**, as in
build systems such as Shake or Salsa). Editing a plotting chunk, a printed
summary, a comment or the formatting then re-runs that chunk only.

*Mechanics.* Planning can no longer classify every chunk up front: a chunk after
an edited one is "pending on its input". Execution resolves it as soon as the
previous fingerprint is known, reusing or running it. Phase 0 still shows such
chunks as modified (muted amber); streaming shows them turning current without
running.

*Obstacles.*
- **Fingerprint cost.** Hashing the state after every chunk. It is almost free
  when the state is already serialized for the snapshot (hash the bytes as they
  are written), and cheaper still with leaf hashes (5.4).
- **Canonical bytes.** Equal states must serialize to equal bytes: fixed
  `PYTHONHASHSEED` (5.1), pickle's deterministic memo; in R, `serialize()` is
  deterministic for data but closures carry bytecode and source references, so
  hash functions by their deparsed body and environment.
- **Unknown state** (section 4): a chunk that only changes, say, `torch`'s seed
  leaves the captured state unchanged, and cutoff would reuse stale results.
  The same chunk already breaks snapshots today, so cutoff adds no new class of
  risk, but it makes it more likely to matter. Mitigations: components for the
  common cases (5.1), the audit (5.2), and a chunk option `#| cutoff: false`
  (on by default, documented with this limitation).

### 5.4 Content-addressed state (resources)

#170: move leaf buffers (protocol-5 buffers in Python; large atomic vectors in
R) out of the snapshots into a store keyed by content hash, shared by every
snapshot. The reference graph stays in one serialization per restore.

It fits the other directions: leaf hashes make fingerprints (5.3) cheap, and a
store of immutable blocks can be shared read-only with the build workspace
instead of copied (#125). Prerequisite: #169.

### 5.5 Adaptive checkpoints (resources, proportional work)

Not every chunk deserves a snapshot. Record each chunk's execution time and
snapshot cost; save a snapshot only when the replay time it saves exceeds its
cost (for example after slow chunks, or every few seconds of computation), and
always at the end of the chain. A miss then restores the nearest earlier
checkpoint and re-executes the cheap chunks between, whose outputs are known.

*Consequences.* Reuse of a chunk's *output* no longer requires its own snapshot
(it needs the nearest checkpoint before the first chunk that must run), which
decouples rendering from snapshot storage. Disk use drops by the ratio of
chunks to checkpoints.

*Obstacles.* Re-executed intermediate chunks must reproduce their outputs; the
audit (5.2) reports those that do not. Durations vary between machines: use
them as a heuristic, never for correctness.

### 5.6 Automatic dependencies (correctness, ergonomics)

- **Python**: an audit hook (`sys.addaudithook`, event `open`) records the files
  a chunk reads and writes. Files read become implicit dependencies (hashed like
  `depends:`); the editor suggests declaring them.
- **R**: no audit hook; wrap the base connection constructors (`file()`,
  `gzfile()`, `url()`) used by `read.csv`, `readRDS`, `load` and the like, in
  the session's environment.
- **Cross-language edges**: a file written by an R chunk and read by a Python
  chunk links the two chains, so that editing the R chunk re-runs the Python
  one. Today the two chains are independent and this goes unnoticed.

*Obstacles.* Reads outside the interpreter's file API (native libraries opening
files themselves, subprocesses, network) are not seen: `depends:` stays, and the
audit reports the rest.

### 5.7 The live loop (research)

- **Warm interpreters** (#76): keep the interpreter alive after a compilation;
  when the next miss starts where the live state is, no restore is needed.
- **In-memory checkpoints by `fork()`** (Unix): fork the interpreter at each
  checkpoint and keep the child suspended; resuming is instantaneous and exact
  (memory sharing, module state and even open resources survive), with
  serialized snapshots as the persistent fallback. Obstacles: memory growth
  (copy-on-write pages diverge), fork safety of multithreaded libraries (BLAS,
  the macOS GUI backends), no `fork()` on Windows. R is fork-friendly
  (`parallel::mcfork`).

## 6. What not to do

- **Split the reference graph** (per-object freezing, per-variable exclusion,
  deduplication at object level): it breaks aliases and cannot be verified
  (#46). Only leaves may leave the graph.
- **Rely on static analysis alone** for dependencies: `eval`, `get`/`assign`,
  `from x import *`, mutation through aliases and side effects make it unsound.
  Use it, if at all, as a hint checked by fingerprints.
- **Degrade silently**: every approximation is either detected (non-reusable,
  replay, warning) or checkable (audit).

## 7. Proposed sequence

1. **0.4.x — correctness.** #167 (done), #169 (detection, R replay marker),
   the process-state registry with built-in components, fixed `PYTHONHASHSEED`,
   and the `caching.md` section on what a restore preserves.
2. **Verification.** `knot build --audit` and differential tests in CI, before
   any optimization, so that the optimizations are measured against the oracle.
3. **Resources.** Adaptive checkpoints (5.5), then leaf deduplication (#170) and
   read-only sharing of the store (#125), measured with `bench_live`.
4. **Proportional work.** State fingerprints and early cutoff (5.3), on top of
   2 and 3.
5. **Dependencies and environment.** I/O tracing (5.6) and package versions
   (5.1), feeding #88.
6. **Research.** Warm interpreters and fork checkpoints (5.7).

## 8. Decisions needed

- Early cutoff on by default, with `#| cutoff: false` as the escape, or opt-in?
- Is `knot build --audit` a separate command, or part of `--strict`?
- A public state-component hook, or only built-in components at first?
- Where adaptive checkpoints draw the line (time saved vs cost), and whether the
  user can pin a checkpoint (`#| checkpoint: true`).
