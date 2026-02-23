# Codebase Hardening — February 2026

**Status:** ✅ Implemented  
**Branch:** `fix/critical-bugs` (merged to master 2026-02-20)  
**Commits:** 21 commits + 1 merge commit

## Overview

A comprehensive audit of the codebase identified 21 issues across four severity
levels. All critical bugs, data loss risks, design flaws, and code quality issues
were resolved. Three performance issues (P1–P3) are deferred to a future sprint.

---

## 🔴 Critical Bugs (C1–C3) — All Resolved

### C1 — Cache `TextAndPlot`/`DataFrameAndPlot` not restored from cache

**File:** `crates/knot-core/src/cache/storage.rs`

`get_cached_result()` now reconstructs the correct variant by inspecting the
number and extensions of files in `entry.files`, instead of always returning
only the first file. `TextAndPlot` and `DataFrameAndPlot` are fully restored.

### C2 — Panic risk on short slice `hash[..8]`

**File:** `crates/knot-core/src/compiler/snapshot_manager.rs`

Replaced unchecked `&hash[..8]` with `&hash[..hash.len().min(8)]` at all four
call sites in `snapshot_manager.rs`. A corrupted or unexpectedly short hash no
longer panics the compilation thread.

### C3 — No timeout on R/Python processes — risk of infinite hang

**File:** `crates/knot-core/src/executors/r/process.rs`, `python/process.rs`

Implemented execution timeout using `Receiver::recv_timeout()` on a dedicated
channel. Default: 30 seconds (configurable via `knot.toml`). When exceeded, the
child process is killed and an error is surfaced to the user.

---

## 🟠 Data Loss (D1–D3) — All Resolved

### D1 — Multiple plots/dataframes silently overwritten

**File:** `crates/knot-core/src/executors/mod.rs`

Adopted "last wins" semantics (a chunk produces one primary result). The
overwrite is now intentional and documented. `MultiPlot` support deferred as
a future feature request with an explicit tracking comment.

### D2 — `StringOrVec::as_str()` returns only first element

**File:** `crates/knot-core/src/executors/side_channel.rs`

Removed `StringOrVec::as_str()` entirely. All callers now use the `Display`
implementation which correctly joins multiple lines with `\n`. Inconsistency
between `Display` and `as_str()` is eliminated.

### D3 — Misspelled options in `knot.toml` silently ignored

**File:** `crates/knot-core/src/config.rs`, `parser/ast.rs`

After `extract_codly_options()`, any key in `other` that does not start with
`codly-` now emits a `log::warn!` with the unknown key name. Users are informed
of typos at compilation time.

---

## 🔵 Design Issues (De1–De5) — All Resolved

### De1 — `start_byte` used as chunk identifier in cache (unstable)

**Files:** `parser/ast.rs`, `parser/winnow_parser.rs`, `compiler/chunk_processor.rs`

Added `index: usize` (0-based ordinal) to the `Chunk` struct, assigned during
parsing. The cache now uses `chunk.index` for all `save_result`, `save_error`,
and naming operations. Cache entries remain stable across document edits that
shift byte positions.

**Impact:** The LSP's `diagnostics.rs` was also updated to match chunks by
`c.index == chunk_cache.index` instead of `c.start_byte == chunk_cache.index`.
A regression in diagnostics/hover was caught and fixed before merge.

### De2 — `Document::parse()` returns `Result` that can never fail

**File:** `crates/knot-core/src/parser/ast.rs`

Changed signature from `pub fn parse(source: String) -> Result<Self>` to
`pub fn parse(source: String) -> Self`. The winnow parser stores errors in
`doc.errors` and always succeeds. All callers (compiler, LSP handlers) updated
to remove the redundant `?` / `unwrap()`.

### De3 — `Option<Option<u32>>` for `digits` in `InlineOptions`

**File:** `crates/knot-core/src/parser/ast.rs`

Added a `[opt]` kind to the `define_inline_options!` macro alongside the
existing `[val]` kind. `digits` is now declared as `[opt] digits: u32` and
stored as `Option<Option<u32>>` with proper macro-generated accessors, so
callers use `Some(3)` instead of the confusing `Some(Some(3))`.

### De4 — Error text hard-coded in French

**File:** `crates/knot-core/src/compiler/mod.rs`

Translated the execution error template (used to generate Typst error blocks)
from French to English for consistency with the rest of the codebase and for
international users.

### De5 — Three places to edit when adding a new language

**Files:** `defaults.rs`, `executors/manager.rs`, `config.rs`

Introduced `pub enum Language { R, Python }` in `defaults.rs` as the single
source of truth. All `match lang` string comparisons were replaced by exhaustive
`match language { Language::R => ..., Language::Python => ... }`. Adding a
new variant to `Language` now produces a compile-time error at every site that
must be updated, eliminating the risk of a silent omission.

---

## 🟣 Code Quality (Q1–Q9) — All Resolved

### Q1 — Double HashMap lookup in `get_executor`

Used the `Entry` API (`entry().or_insert_with()`-style) to unify the
`contains_key` + `get_mut` pair into a single lookup using
`Entry::Occupied` / `Entry::Vacant`.

### Q2 — Duplication: `format_codly_call` and `format_local_call`

Extracted a private `format_typst_call(fn_name, options)` helper. Both public
functions now delegate to it, differing only by the Typst function name.

### Q3 — Duplicated test helpers across compiler modules

Created `crates/knot-core/src/compiler/test_helpers` (`#[cfg(test)]`,
`pub(super)`). `setup_test_cache()` and `setup_test_manager()` are now defined
once and imported in both `chunk_processor` and `inline_processor`.

### Q4 — Implicit lock ordering in LSP — future deadlock risk

Added a `# Lock ordering` doc-comment to `ServerState` in
`crates/knot-lsp/src/state.rs` establishing the canonical acquisition order:
`documents → tinymist → executors → formatter → (path overrides)`.
Verified that no handler currently holds two locks simultaneously.

### Q5 — `hash_dependencies` uses mtime (insufficient resolution on some FS)

`hash_dependencies()` now reads and SHA256-hashes the full file content instead
of relying on `mtime + size`. This correctly detects rapid changes (sub-second
on HFS+, sub-2-second on FAT32). The test that used `thread::sleep(10ms)` to
work around mtime granularity was updated to remove the sleep.

### Q6 — Asymmetric integrity check between R and Python

Both `RExecutor::load_constant()` and `PythonExecutor::load_constant()` already
performed the same SHA256 integrity check at the time of the audit. No change
needed.

### Q7 — `TypstBackend::new()` instantiated per chunk

`TypstBackend` is a zero-size struct. Moved instantiation to `Compiler::compile()`
(once per compilation) and passed `&backend` as a parameter to `process_chunk()`.

### Q8 — `Cache::get_chunk_hash`: unnecessary indirection

Removed the `get_chunk_hash` method on `Cache` (which only delegated to the
free function). Made `cache::hashing` module `pub` and updated all call sites
to use `hashing::get_chunk_hash(...)` directly.

### Q9 — O(n) round-trips for constant object verification

Added `hash_objects_batch(names)` to both `resources/python/constants.py` and
`resources/r/constants.R`. Added `fn hash_objects(&mut self, names: &[String])`
to the `ConstantObjectHandler` trait with a default N-round-trip implementation.
Python and R executors override with a single JSON batch query. `get_constants_hash()`
now calls `exec.hash_objects(constants)?` (1 round-trip) instead of looping
over `exec.hash_object(var)` (N round-trips).

---

## Dependency Updates (chore/update-dependencies)

Applied in a separate branch merged immediately after:

| Crate | Before | After | Crates |
|-------|--------|-------|--------|
| `toml` | 0.8.19 | 1.0.3 | knot-core, knot-cli |
| `which` | 6.0.3 | 8.0.0 | knot-lsp |
| `anyhow` | 1.0.101 | 1.0.102 | — |
| `clap` | 4.5.58 | 4.5.60 | — |
| `syn` | 2.0.116 | 2.0.117 | — |

No code changes were required for `toml` v1 or `which` v8 given our usage
patterns (deserialisation only for `toml`; single `which::which()` call for
`which`).

---

## Deferred Issues

The following performance issues were identified but deferred:

- **P1** — Cache O(n) scans; replace `Vec<ChunkCacheEntry>` with `HashMap`
- **P2** — `save_metadata()` called once per chunk; defer to end of compilation
- **P3** — `Cache::new()` recreated from disk on every `did_save` in LSP
