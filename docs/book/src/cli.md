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

## knot watch

```bash
knot watch [--preview]
```

Watches all `.knot` files in the project for changes. On every save:
1. Re-compiles changed chunks (using the cache for unchanged ones).
2. Writes the updated `.typ` file.
3. The background `typst watch` process picks up the new `.typ` and regenerates
   the PDF automatically.

With `--preview`, uses `tinymist preview` instead of `typst watch`, opening a
browser preview.

> For a richer live preview experience (streaming, per-chunk updates, sync),
> use the [VS Code extension](./vscode.md) instead.

## knot compile

```bash
knot compile <file.knot>
```

Compiles a single `.knot` file to a `.typ` file (no PDF). Useful for debugging
or scripting.

## knot clean

```bash
knot clean
```

Removes the `.knot_cache/` directory and all generated `.typ` files. The next
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

The CLI formats R blocks with **Air** and Python blocks with **Ruff**, found on
`PATH`, and normalizes Knot chunk headers/options. It preserves Typst text and
inline expressions, including their execution options. Typst formatting through
Tinymist is provided by the editor, not this command. Neither R nor Python code
is executed during formatting.

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
