# VS Code

The Knot VS Code extension provides a complete editing environment for `.knot`
files: live preview, bidirectional sync, completion, diagnostics, and formatting.

## Starting the preview

Open a `.knot` file and click **Start Preview** in the status bar, or use the
command palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) and run **Knot: Start Preview**.

A browser window opens with a live, streaming preview of your document.

## The preview lifecycle

Understanding what happens when you interact with the document helps you make the
most of the preview:

### While typing (before saving)

The preview updates **instantly** using only cached results. Modified chunks —
those whose code you have changed since the last compile — show an **amber border**:

- **Amber (strong border)**: the chunk you edited directly.
- **Amber (thin border)**: chunks downstream of the edit whose cache is
  invalidated by the chained hash cascade (they haven't been re-executed yet,
  but they will be when you save).

No code executes while you type. The preview is pure Typst — immediate.

### On save

1. **Phase 0** (< 50 ms): Knot assembles a preview using cached outputs for
   unchanged chunks, and **orange** placeholders for chunks that need to
   re-execute. This appears before any code runs.

2. **Streaming execution**: Each chunk executes in sequence. As each one
   finishes, its result replaces the orange placeholder in real time. You see
   results appear one by one, not all at once.

3. **Final**: once all chunks have executed, diagnostics are refreshed.

### What the border styles mean

| Style | Meaning |
|---|---|
| None | Output is current (cache hit or just executed) |
| 5 pt amber dotted | Chunk you edited directly — awaiting save |
| 1 pt amber dashed | Downstream hash-cascade invalidation — awaiting save |
| 2 pt orange solid | Compile in progress — chunk queued for execution |
| White semi-transparent overlay | Inert — execution suspended due to an upstream error |

These styles are defined in `lib/knot.typ` via the `knot-state-styles` dictionary
and can be customised per project:

```typst
// In lib/knot.typ — override any entry to change the preview appearance
#let knot-state-styles = (
  pending: (stroke: 2pt + rgb("#f97316")),
  modified: (stroke: (thickness: 5pt, paint: rgb("#fcd34d"), dash: "densely-dotted")),
  "modified-cascade": (stroke: (thickness: 1pt, paint: rgb("#fcd34d"), dash: "dashed")),
  inert: (overlay-fill: white.transparentize(40%)),
)
```

> These styles appear **only in the live preview** — they never show up in the final PDF.

## Bidirectional sync

**Forward sync** (source → PDF): move your cursor in the `.knot` editor. The PDF
preview scrolls to the corresponding position automatically.

**Backward sync** (PDF → source): click anywhere in the PDF preview. VS Code opens
the corresponding line in the `.knot` source file.

## Completion and hover

- **Chunk options**: type `#| ` inside a chunk to see completions for all available
  options. Hover over an option name to read its documentation.
- **Typst symbols**: completion and hover for Typst functions and variables is
  provided by Tinymist (proxied through Knot's LSP).
- **R and Python**: hover over identifiers in code chunks to see type information
  (provided by Tinymist for the virtual `.typ` representation).

## Formatting

Save with formatting enabled (`editor.formatOnSave: true`) to format:

- **R code** with [Air](https://posit-dev.github.io/air)
- **Python code** with [Ruff](https://docs.astral.sh/ruff)
- **Typst source** with Tinymist's built-in formatter

Each formatter is invoked only if the corresponding binary is available in PATH.
Knot reconstructs the `.knot` file from the formatted parts without touching the
other sections.

## Diagnostics

Errors appear in the Problems panel and as inline squiggles:

- **Parse errors**: malformed `#|` options, unknown option names.
- **Runtime errors**: R or Python exceptions from the last compile. These persist
  in the cache so they are visible even before you trigger a new compile.
- **Typst errors**: forwarded from Tinymist.

## Extension settings

| Setting | Default | Description |
|---|---|---|
| `knot.tinymistPath` | (PATH) | Path to the `tinymist` binary if not in PATH. |
| `knot.airPath` | (PATH) | Path to the `air` binary if not in PATH. |
| `knot.ruffPath` | (PATH) | Path to the `ruff` binary if not in PATH. |
