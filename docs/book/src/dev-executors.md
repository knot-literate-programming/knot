# Language Executors

Both language executors live in `crates/knot-core/src/executors/` and follow
the same pattern. Understanding this pattern is the prerequisite for adding a
new language.

---

## How an executor works

1. **Spawn a subprocess** running a persistent interpreter (R or Python).
   The interpreter loads an embedded helper script on startup
   (`resources/typst.R` or `resources/typst.py`), which defines the `typst()`
   and `current_plot()` functions.

2. **Before each execution**, set environment variables in the child process:

   | Variable | Purpose |
   |---|---|
   | `KNOT_METADATA_FILE` | Path to the side-channel JSON temp file |
   | `KNOT_CACHE_DIR` | Cache directory (for saving plot files) |
   | `KNOT_FIG_WIDTH` | Figure width in inches |
   | `KNOT_FIG_HEIGHT` | Figure height in inches |
   | `KNOT_FIG_DPI` | Dots per inch (for raster formats) |
   | `KNOT_FIG_FORMAT` | Output format: `svg` or `png` |

   These **must** be set in the **child** process environment, not the Rust
   process. In R: `Sys.setenv(KNOT_METADATA_FILE = ...)`. In Python:
   `os.environ["KNOT_METADATA_FILE"] = ...`.

3. **Send the user code** to the interpreter's stdin, followed by a sentinel.

4. **Read stdout/stderr** until the sentinel appears.

5. **Read the side-channel JSON** (`KNOT_METADATA_FILE`) for rich output —
   plot file paths, DataFrame data, etc.

6. **Return an `ExecutionAttempt`**: `Success(ExecutionOutput)` or
   `RuntimeError(RuntimeError)`.

---

## The side-channel

The side-channel (`executors/side_channel.rs`) is a temporary JSON file that
lets the language runtime pass structured metadata back to Rust without
shell-escaping issues. After each chunk:

- The helper script writes to the JSON file if `typst()` or `current_plot()`
  was called.
- The Rust executor reads and clears the file.
- The result becomes part of `ExecutionOutput.outputs`.

---

## The `SnapshotManager`

`SnapshotManager` (`compiler/snapshot_manager.rs`) wraps an executor and adds
environment snapshotting:

- **`save_snapshot(chunk_hash)`** — serialises the interpreter environment to
  a file in `.knot_cache/snapshots/`.
- **`restore_snapshot(chunk_hash)`** — restores a previously saved environment.

This is what allows re-executing chunk 5 without re-running chunks 1–4:
Knot restores the snapshot from just before chunk 5 ran, then re-executes
chunk 5.

---

## Adding a new language executor

Here is the checklist. All steps are in `crates/knot-core/`.

### 1. Add a helper script

Create `resources/<lang>/typst.<ext>` with at least a `typst()` function.
The function should:
1. Check if `KNOT_METADATA_FILE` is set.
2. Serialise its argument (text, DataFrame, or plot) to the side-channel.
3. Fall back to printing if the variable is not set.

### 2. Create the executor

Create `src/executors/<lang>/` with:

- `mod.rs` — re-exports
- `execution.rs` — implements `Executor`:

```rust
pub trait Executor: Send {
    fn execute(
        &mut self,
        code: &str,
        options: &GraphicsOptions,
        cache_dir: &Path,
    ) -> Result<ExecutionAttempt>;

    fn save_environment(&mut self, path: &Path) -> Result<()>;
    fn restore_environment(&mut self, path: &Path) -> Result<()>;
}
```

### 3. Register the language

- Add a variant to `Language` in `executors/mod.rs`.
- Add a case to `ExecutorManager::spawn` in `executors/manager.rs`.
- Add a case to `group_by_language` in `compiler/execution.rs`.

### 4. Add parser support

- Add the language identifier to the chunk fence parser in
  `parser/winnow_parser.rs` (the `language` combinator).

### 5. Add options

If your language needs language-specific default options, add a section to
`ChunkOptions` in `parser/options.rs` and to `knot.toml` parsing in
`config.rs`.

### 6. Write tests

Add snapshot tests in `executors/<lang>/` and integration tests (marked
`#[ignore]` unless R/Python-free) in a `tests/` module.
