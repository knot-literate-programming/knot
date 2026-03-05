# The Three-Pass Pipeline

The compiler lives in `crates/knot-core/src/compiler/`. Understanding the three
passes is the prerequisite for almost any change to how Knot executes code.

---

## Pass 1 — Planning (`pipeline.rs`)

**Input**: parsed `Vec<Node>` + cache
**Output**: `Vec<PlannedNode>` — every node annotated with `ExecutionNeed`

Planning does four things:

1. **Resolve options** for each chunk: merge global defaults, language defaults,
   and per-chunk `#|` options into a `ResolvedChunkOptions`.

2. **Compute a chained SHA-256 hash** for each chunk in each language chain.
   The hash of chunk N covers:
   - the chunk's source code
   - its resolved options
   - the hash of chunk N-1 in the same language

   Because hashes chain, editing chunk 3 changes the hash of chunk 4, 5, 6, …
   even if their code is unchanged. This guarantees downstream re-execution.

3. **Classify each chunk**:
   - `Skip` — `eval: false` option
   - `CacheHit(attempt)` — hash matched a cache entry
   - `MustExecute` — hash not in cache (new or changed)

4. **Apply Phase0Mode** when assembling the partial document (for live preview):
   - `Phase0Mode::Pending` (during `do_compile`): all `MustExecute` chunks get
     `ChunkExecutionState::Pending` (orange border).
   - `Phase0Mode::Modified` (during `do_phase0_only`): the first `MustExecute`
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

1. Iterate over `MustExecute` nodes in document order.
2. For each node, call the executor (`RExecutor` or `PythonExecutor`).
3. Write the result to cache (success or error).
4. If the result is an error, all subsequent nodes in the chain become `Inert`
   (interpreter state is uncertain).
5. If `freeze` objects are declared, `check_freeze_contract` is called after
   each subsequent `MustExecute` node — a hash mismatch also cascades `Inert`.

The `ExecutorManager` uses a take/put-back pattern so executors can be moved
into threads without lifetime issues.

`SnapshotManager` saves and restores interpreter state (the R/Python environment
just before each chunk). This allows re-executing chunk 5 in a 20-chunk document
without re-running chunks 1-4.

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

For most purposes you will use the project-level API in `project.rs`:

```rust
// One-shot compilation (used by knot build / knot watch)
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
