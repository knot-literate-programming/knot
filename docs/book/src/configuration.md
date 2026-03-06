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
timeout_secs = 60

[chunk-defaults]
# Default options applied to every chunk in every language.
# All chunk options are valid here.
echo = true
warning = true
fig-width = 6
fig-height = 4
fig-format = "svg"

[r-chunks]
# Defaults applied to R chunks only (override [chunk-defaults]).
warning = false

[python-chunks]
# Defaults applied to Python chunks only (override [chunk-defaults]).
fig-dpi = 200

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
