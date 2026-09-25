# Live-loop performance baseline (#71)

Measured before any optimisation, to decide which performance work v0.4 (#110)
needs. Reuse these figures for #72–#76.

## Method

```bash
cargo run --release -p knot-core --example bench_live -- \
    [--inflate-mb N] [--runs N] [--main anscombe-python.knot]
```

The benchmark (`crates/knot-core/examples/bench_live.rs`) copies
`examples/anscombe` to a temporary directory and follows the editor paths:

- **Keystroke**: `ProjectBuild::prepare` with the unsaved buffer, Phase 0 and
  a non-final publication, which the LSP runs on every debounced change
  (300 ms).
- **Save**: prepare, full compilation and final publication. Streamed updates
  are published as the LSP does, and their cost is measured per update.

`--inflate-mb N` binds an N MiB object in the first Python chunk, so every
Python snapshot after it holds N MiB, like a session holding a large data
frame. Anscombe has four Python snapshots, so N MiB gives about 4 × N MiB of
snapshots. Debug timings of each build step are logged with
`RUST_LOG=knot_core=debug`.

Machine: Apple M1 Pro, 16 GiB, APFS SSD, macOS 26.6; rustc 1.98.1, Python
3.12, R 4.6.1. Medians of 5 to 10 runs.

## Results

### Keystroke (Phase 0 with an unsaved buffer)

| Snapshots | prepare (copy) | Phase 0 | publish | **total** |
|---|---|---|---|---|
| 0.4 MiB (Anscombe as shipped) | 9 ms | 5 ms | 4 ms | **18 ms** |
| 400 MiB (`--inflate-mb 100`) | 352 ms | 1 254 ms | 6 ms | **1.6 s** |
| 2 000 MiB (`--inflate-mb 500`) | 1 701 ms | 6 299 ms | 4 ms | **8.0 s** |

Phase 0 time is spent re-hashing snapshots: `Cache::snapshot_is_valid`
computes the SHA-256 of every snapshot file of every cached node, on every
planning pass. With that check replaced by an existence test (experiment
only, not committed), Phase 0 at 400 MiB drops from 1 254 ms to 4 ms and the
keystroke total from 1.6 s to 0.38 s; the rest is the workspace copy.

### Save

| Snapshots | prose edit (cache hits) | one Python chunk edited |
|---|---|---|
| 0.4 MiB | 31 ms | 1.1 s |
| 400 MiB | 2.0 s | 4.2 s |
| 2 000 MiB | 10.4 s | 16.6 s |

A save with only cache hits pays the copy into the workspace, the snapshot
re-hashing, and a final publication that copies every cache file back, changed
or not. Editing a chunk adds the interpreter start, the snapshot restore and
the execution.

### Streaming

With Anscombe as shipped, **no update is streamed**: all chunks are in
includes, and includes are compiled completely before the main file, whose
chunks alone are streamed. The preview shows the Phase 0 placeholders, then
everything at once.

With the chunks in the main file (`--main anscombe-python.knot`), each of the
4 streamed updates costs about 1 ms to publish, for prose-only and chunk
edits alike.

## Decision for v0.4

Without large sessions the loop is fast (18 ms per keystroke). It degrades
linearly with the total size of snapshots, which reaches hundreds of MiB as
soon as a session holds a large data frame across several chunks; the
default snapshot warning threshold is 1 GB per language and file.

- **#73 (cheap snapshot validation): include.** It is 78 % of the keystroke
  time with large snapshots. It does not require a cache-format bump: the
  size and modification time can be optional fields of `SnapshotEntry`
  (`#[serde(default)]`), with the full hash still checked at restore. This
  moves #73 out of the cache v3 coordination.
- **#72 (read-only Phase 0): include.** It removes the remaining workspace
  copy from the keystroke path (0.35 s per 400 MiB). With #73 the keystroke
  cost then no longer depends on snapshot size.
- **#74 (incremental artifact publication): defer.** Streamed publication
  costs about 1 ms per update on Anscombe.
- **Follow-ups, not v0.4 blockers:**
  - the final publication copies every cache file back, even unchanged ones;
  - include-based projects are not streamed (see above).

## After #73 and #72

Same machine and benchmark. The keystroke path now uses
`ProjectBuild::prepare_preview`, like the LSP: no workspace copy, and
snapshots are validated by size and modification time.

| Snapshots | keystroke before | keystroke after | prose-only save before → after | one Python chunk before → after |
|---|---|---|---|---|
| 0.4 MiB | 18 ms | **2 ms** | 31 → 32 ms | 1.1 → 1.1 s |
| 400 MiB | 1.6 s | **2 ms** | 2.0 → 0.83 s | 4.2 → 3.5 s |
| 2 000 MiB | 8.0 s | **2 ms** | 10.4 → 4.0 s | 16.6 → 15.8 s |

The keystroke cost no longer depends on the size of the caches. Saves still
copy the caches into the build workspace and every cache file back on final
publication (#125); a chunk edit also pays the interpreter start, the restore
of the previous snapshot and the execution.
