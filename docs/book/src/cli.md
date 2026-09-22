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
knot format <file.knot>
knot format <file.knot> --check
```

Formats a `.knot` file using Air (R), Ruff (Python), and Tinymist (Typst).
With `--check`, exits with a non-zero status if the file would be reformatted
(useful in CI).

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
