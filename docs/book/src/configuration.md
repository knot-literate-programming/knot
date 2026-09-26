# knot.toml

`knot.toml` is the project configuration file. Knot searches for it by walking
up from the current directory. Place it at the project root.

## Minimal configuration

```toml
[document]
main = "main.knot"
```

## Full reference

```toml
[document]
# Main document file (required)
main = "main.knot"

# Additional .knot files compiled before main and injected at
# /* KNOT-INJECT-CHAPTERS */ in main.knot (or at the end if missing).
includes = ["chapter1.knot", "chapter2.knot"]

[execution]
# Abort chunk execution after this many seconds (default: 30)
timeout-secs = 60

[chunk-defaults]
# Default options applied to every chunk in every language.
# Chunk options are valid here, except caption (see the "In knot.toml"
# column of the chunk options reference).
show = "both"
warnings-visibility = "below"
fig-width = 6
fig-height = 4
fig-format = "svg"

[r-chunks]
# Defaults applied to R chunks only (override [chunk-defaults]).
warnings-visibility = "none"

[python-chunks]
# Defaults applied to Python chunks only (override [chunk-defaults]).
dpi = 200

[codly]
# Options passed to the codly Typst package for syntax highlighting.
# These apply globally to all code blocks.
# Refer to the codly documentation for available keys.
# Example:
# zebra-fill = "luma(250)"
```

## Option precedence

From lowest to highest priority:

1. Built-in Knot defaults
2. `[chunk-defaults]` in `knot.toml`
3. `[r-chunks]` or `[python-chunks]` in `knot.toml`
4. Per-chunk `#|` options in the `.knot` file

## Unknown keys

Knot warns about keys and sections it does not recognise, which are usually
typos, and suggests the closest known name:

```text
warning: knot.toml: unknown key 'fig-widht' in [chunk-defaults] is ignored. Did you mean 'fig-width'?
```

The warning is printed by `knot build` and `knot compile`, shown in the editor on
the first line of the main document, and listed at the end of the PDF. The
ignored key does not affect the rest of the configuration, and warnings never
fail a build, even with `--strict`. `[codly]` accepts any key (they are passed to
codly), and chunk sections accept any `codly-*` key. The former `[helpers]`
section is no longer used: remove it.

## External tools

An optional `[tools]` section selects the executables used by both the CLI and
the language server:

```toml
[tools]
python = './.venv/bin/python'
r = '/usr/local/bin/R'
typst = './tools/typst'
tinymist = './tools/tinymist'
air = './tools/air'
ruff = './.venv/bin/ruff'
```

Each entry is optional. Use only the entries you need, with paths appropriate to
your installation. On Windows, for example:

```toml
[tools]
python = '.venv\Scripts\python.exe'
r = 'C:\Program Files\R\R-4.5.0\bin\R.exe'
```

TOML single-quoted strings preserve backslashes. Paths containing spaces need no
additional quoting inside the string. Relative paths are resolved from the
folder containing `knot.toml`, even when compiling or formatting a file in a
subdirectory. A bare name such as `python = 'python3.12'` is looked up in `PATH`;
use `./python` to select a file in the project directory. Values identify one
executable, not a shell command: arguments, `~` and environment variables are
not expanded.

`r` must point to the **interactive R executable**, not `Rscript`: Knot maintains
a persistent interpreter session across chunks. Python uses the selected
interpreter's environment, so a virtual environment can be selected without
activating it in the editor.

Resolution order is:

1. The explicit entry in `[tools]`. An invalid entry produces an error without
   falling back to a different installation.
2. The default command in `PATH`: `python3`, `R`, `typst`, `tinymist`, `air`, or `ruff`.
3. Common installation directories: `~/bin`, `~/.cargo/bin`, `/usr/local/bin`
   and `/opt/homebrew/bin` on Unix; the home directories and
   `%LOCALAPPDATA%/Programs/<command>` on Windows.
4. For LSP formatters and Tinymist, a path supplied by the editor through
   `initializationOptions` (`airPath`, `ruffPath`, `tinymistPath`).

The VS Code client supplies available binaries from the installed Tinymist, Air
and Ruff extensions. Its Air/Ruff path settings override those bundled candidates,
but remain fallbacks after project settings and local discovery. Other LSP
clients can supply the same initialization options; the server does not scan
editor installation directories.

Tool lookup is lazy: a project without R chunks does not need R, and formatting
plain Typst does not require Air or Ruff. Interpreter and formatter settings are
read on the next compilation or formatting request. Restart the language server
after changing `tools.tinymist`. The server selects Tinymist from the first
workspace folder (or `rootUri`), so open the project folder containing
`knot.toml` to use its Tinymist setting. Restart `knot watch` after changing its
preview executable.

Changing the configured R or Python path invalidates that language's execution
cache, including inline expressions and snapshots. Updating packages or replacing
an interpreter at the same path is not detected automatically; run `knot clean`
after such changes. Changing a rendering or formatting tool does not invalidate
execution caches.
