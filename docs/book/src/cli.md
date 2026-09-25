# CLI Reference

The `knot` command-line tool manages compilation, watching, and project
initialisation. Run `knot --help` or `knot <command> --help` for the full
list of flags.

## knot init

```bash
knot init <name>
```

Creates a new project directory `<name>` with a `knot.toml`, a `main.knot`
template, and the `lib/knot.typ` helper file.

## knot build

```bash
knot build
```

Compiles the project to a `.typ` file, then calls `typst compile` to produce a
PDF. Reads `knot.toml` from the current directory or any parent directory.

Typst errors are printed on stderr with their locations and help text, and the
command exits unsuccessfully. Locations refer to the generated `.typ` file.
Warnings are also printed on stderr when compilation succeeds. If Typst cannot
be launched, the error identifies the command and suggests checking `PATH`.
In VS Code, build failures open the Knot output channel with the diagnostics;
warnings from successful builds are retained in that channel.

Errors in the document itself (a failing chunk, an invalid option, an invalid
YAML header, an unclosed chunk, a missing include, an interpreter that cannot
start) do not stop the build: the PDF shows each of them where it occurs, and
`knot build` lists them on stderr as `file:line: message`. The exit status is
successful unless `--strict` is given.

### Options

- `--no-snapshots` — re-execute every language chain in fresh interpreters,
  without saving or restoring session snapshots.
- `--strict` — exit unsuccessfully when the document shows errors. The PDF is
  still written, so you can read the errors in context. Warnings (for example
  an ignored option) and display-only code (`eval: false`) do not fail the build.

### Validating a final document

```bash
knot build --strict --no-snapshots
```

This is the recommended check before publishing a document, and in CI. Every
chunk and inline expression that should run is executed again, in document
order, in fresh R and Python sessions: no cached result or snapshot replaces
an execution. The command succeeds only if the whole document executed
without errors.

It validates one complete execution of your code, not its environment. It does
not pin package or interpreter versions, freeze input data, fix random seeds
or reproduce network services. Record those separately (for example with a
lockfile, versioned data and explicit seeds) when you need them.

## knot watch

```bash
knot watch [--preview]
```

Watches the files that make up the document: `knot.toml`, the main file, the
includes (including a listed include that does not exist yet) and the files
declared with `depends:` in any chunk. Changes to other files, such as the
generated `.typ` or `_knot_files/`, are ignored. On every change:
1. Re-compiles changed chunks (using the cache for unchanged ones).
2. Writes the updated `.typ` file and lists the document's errors and
   configuration warnings, as `knot build` does.
3. The background `typst watch` process picks up the new `.typ` and regenerates
   the PDF automatically.

Changes are grouped: Knot waits for a short quiet period (200 ms) after the last
change before compiling, so saving several files, or an editor that saves
through a temporary file, causes a single rebuild, and a change made during a
compilation triggers the next one. The watched files are recomputed after each
build, so adding an include or a `depends:` entry takes effect immediately. If
`knot.toml` becomes invalid, the previous list is kept until it is fixed.

With `--preview`, uses `tinymist preview` instead of `typst watch`, opening a
browser preview.

> For a richer live preview experience (streaming, per-chunk updates, sync),
> use the [VS Code extension](./vscode.md) instead.

## knot compile

```bash
knot compile <file.knot>
```

Compiles a single `.knot` file to a self-contained `.typ` file (no PDF), as a
document of its own: the project's includes are not injected, but `knot.toml`
still applies. Useful for debugging or scripting.

The output is written to `.<stem>.typ` at the project root (for example
`knot compile chapters/intro.knot` writes `.intro.typ`), next to the generated
`_knot_files/` directory, so it never replaces the output of `knot build`. It
embeds the Knot Typst library and compiles as is:

```bash
typst compile --root . .intro.typ
```

## knot clean

```bash
knot clean
```

Removes `.knot_cache/`, `_knot_files/`, the main document's generated `.typ`
and `.pdf` files, and hidden `.typ`/`.pdf` intermediates at the project root.
Reports the number of cached chunks invalidated (across all documents) and files
removed, or `Nothing to clean.` when there are no files to remove. Empty cache
and helper directories are also removed. If a cache index is unreadable, cleanup
still proceeds and the chunk count is reported as unavailable. The next
`knot build` or `knot watch` will re-execute all chunks from scratch.

## knot format

```bash
knot format [file.knot]
knot format [file.knot] --check
```

With a file argument, formats that file without requiring a project. Without an
argument, finds `knot.toml` from the current directory or its parents and formats
`document.main` plus the declared `document.includes`. Duplicate paths are handled
once; unrelated files are not scanned. Declared sources must remain inside the
project root.

The CLI and the editor use the same document formatter: **Air** for R blocks,
**Ruff** for Python blocks, and embedded **Typstyle 0.15.1** for Typst text.
Air/Ruff are found on `PATH` by the CLI; no Typstyle executable or Tinymist process
is needed for formatting. Neither R nor Python code is executed.

Document style is fixed across both clients: two spaces, width 80, no prose
wrapping and no import reordering. Editor tab settings and Tinymist formatter
settings do not override this Knot document style. Use the same Air/Ruff versions
in your terminal and editor for matching code-block results.

Knot protects executable blocks and inline expressions before Typst formatting,
then restores them only if all placeholders remain intact and in order. Inline
code and options are preserved verbatim; block headers/options are normalized.
Typst syntax errors or reconstruction failures abort formatting without writing.

`--check` prints `Would format <path>` for each changed file and writes nothing.
It exits with **0** when all selected files are already formatted, or **1** when
changes are needed. Missing files, invalid configuration, detected chunk syntax errors,
and unavailable or failing formatters also return **1**, with an error on stderr.
A normal formatting run prints `Formatted <path>` for changed files.

All selected files are read and formatted before writing starts. A formatter or
parse failure therefore leaves all sources unchanged. A filesystem failure during
the write phase can still leave earlier files formatted.

## knot jump-to-source

```bash
knot jump-to-source <main.typ> <line> [--open | --json]
```

Maps a line number in the compiled `.typ` file back to the corresponding line in
the `.knot` source. Prints `file:line` to stdout.

With `--open`, opens VS Code at that position via `code --goto`.

Used internally by the VS Code extension for backward sync (PDF → source).

## knot jump-to-typ

```bash
knot jump-to-typ <main.typ-or-project-directory> <file.knot> <line> [--json]
```

Maps a line number in a `.knot` source file to the corresponding line in the
compiled `.typ` file. Prints the line number to stdout.

A project directory resolves the generated file using `document.main` from
`knot.toml`, including when the main source is in a subdirectory. The source
argument is relative to the project root.

Both commands accept **1-based** line numbers. With `--json`, stdout contains
`{"file":"absolute path","line":12}`. The extension uses this structured format
to preserve spaces, Unicode and Windows drive letters. Without it, the text
formats above remain unchanged. Unmapped positions return a non-zero exit code.

Used by the VS Code command for source → generated Typst navigation; automatic
source → PDF scrolling uses the LSP and the same core line mapper.
