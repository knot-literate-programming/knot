# The Three-Pass Pipeline

The compiler lives in `crates/knot-core/src/compiler/`. Understanding the three
passes is the prerequisite for almost any change to how Knot executes code.

---

## Pass 1 — Planning (`mod.rs`; types in `pipeline.rs`)

**Input**: parsed `Vec<Node>` + cache
**Output**: `Vec<PlannedNode>` — every node annotated with `ExecutionNeed`

Planning does four things:

1. **Resolve options** for each chunk: merge global defaults, language defaults,
   and per-chunk `#|` options into a `ResolvedChunkOptions`.

2. **Compute a chained SHA-256 hash** for each chunk in each language chain.
   The hash of chunk N covers:
   - the chunk's source code
   - its language, execution options, dependencies and script version
   - the hash of chunk N-1 in the same language

   Because hashes chain, editing chunk 3 changes the hash of chunk 4, 5, 6, …
   even if their code is unchanged. This guarantees downstream re-execution.

3. **Classify each chunk**:
   - `Skip` — `eval: false` option
   - `CacheHit(attempt)` — matching result and restorable snapshot are intact (cached runtime errors need no snapshot)
   - `MustExecute` — missing/invalid cache, disabled reuse, or an upstream node that must execute
   - `Rejected` — invalid chunk options or a missing `depends` file: not executed, and the language chain is
     suspended as after a runtime error

4. **Apply Phase0Mode** when assembling the partial document (for live preview):
   - `Phase0Mode::Pending` (on save or Run): all `MustExecute` chunks get
     `ChunkExecutionState::Pending` (orange border).
   - `Phase0Mode::Modified` (while typing): the first `MustExecute`
     per language chain gets `Modified` (amber thick), subsequent ones get
     `ModifiedCascade` (amber thin) — distinguishing direct edits from
     hash-cascade invalidations.

---

## Pass 2 — Execution (`execution.rs`)

**Input**: `Vec<PlannedNode>` (only `MustExecute` nodes are processed)
**Output**: `Vec<ExecutedNode>` — results written to cache

Execution uses `std::thread::scope` to run R and Python chains in parallel:

```
group_by_language(planned_nodes)
  ├── R chain   → thread A → run_language_chain(r_nodes)
  └── Python chain → thread B → run_language_chain(python_nodes)
```

Within each `run_language_chain`:

1. The planner reads document YAML `snapshots` settings. Disabled languages have
   every non-skipped node marked `MustExecute`.
2. Each disabled chain runs in a fresh interpreter without saving or restoring
   snapshots. Other chains reuse cached results and restore valid snapshots.
3. Runtime errors stop subsequent nodes of that language (`Inert`).
4. Cache successful results and save a complete session snapshot only when enabled.

The `ExecutorManager` uses a take/put-back pattern so executors can be moved
into threads without lifetime issues.

`SnapshotManager` saves and restores interpreter state (the R/Python environment
after each successfully executed node). This allows re-executing chunk 5 in a 20-chunk document
without re-running chunks 1-4, provided that snapshots are enabled for the language.

---

## Pass 3 — Assembly (`mod.rs`)

**Input**: original `Vec<Node>` + execution results + cache
**Output**: a `.typ` string

Assembly interleaves prose and code-chunk outputs in document order. For each
node it calls `format_node()` in `backend.rs`, which:

- For prose: emits the text as-is (it is already valid Typst).
- For code chunks: calls `format_chunk()` to wrap the output in the appropriate
  `#code-chunk(...)` call with the right state flags and options.
- Embeds `#KNOT-SYNC` markers for bidirectional source ↔ PDF navigation.

---

## Streaming (two-phase API)

The compiler exposes a two-phase API on the `Compiler` struct for live preview:

```rust
// Phase 0: planning only (no code executed), returns partial .typ immediately
fn plan_and_partial(
    &self,
    nodes: Vec<Node>,
    mode: Phase0Mode,
) -> Result<(Vec<PlannedNode>, Arc<Mutex<Cache>>, String)>

// Phase 1: execution + streaming
fn execute_and_assemble_streaming(
    &self,
    planned: Vec<PlannedNode>,
    cache: Arc<Mutex<Cache>>,
    progress: Option<Sender<ProgressEvent>>,
) -> Result<String>
```

`ProgressEvent` carries the `doc_idx` of the completed node plus its
`ExecutedNode`. The LSP uses these to rebuild the `.typ` string and push
incremental updates to Tinymist after each chunk completes.

---

## Entry points

The CLI and the LSP use `ProjectBuild` (`project/build.rs`):

```rust
// Capture sources (with open buffers) and copy the caches into a private
// workspace; nothing shared is modified until `publish`.
ProjectBuild::prepare(root, &buffers, cancellation) -> Result<ProjectBuild>
// Same capture without copying the caches: Phase 0 only (typing).
ProjectBuild::prepare_preview(root, &buffers, cancellation) -> Result<ProjectBuild>

build.phase0(mode) -> Result<ProjectOutput>
// `on_progress` receives the complete output after each executed chunk.
build.compile(on_progress) -> Result<ProjectOutput>
// Writes main.typ; `complete` also publishes caches and artifacts atomically.
build.publish(&output, complete) -> Result<()>
```

`project.rs` keeps thin wrappers over it:

```rust
pub fn compile_project_full(
    root: &Path,
    on_progress: Option<Box<dyn Fn(String) + Send>>,
) -> Result<ProjectOutput>

// Phase 0 only (instant, used by LSP on didChange)
pub fn compile_project_phase0(root: &Path, mode: Phase0Mode) -> Result<ProjectOutput>

// Phase 0 with unsaved buffer (typing-time updates)
pub fn compile_project_phase0_unsaved(
    root: &Path,
    unsaved_path: &Path,
    content: &str,
    mode: Phase0Mode,
) -> Result<ProjectOutput>
```

See [Cache and Execution State](./caching.md) for isolation, repair, snapshot policy, and
non-serializable Python state.
