# README demo

`docs/book/src/images/demo.gif` (and `demo.mp4`) is recorded from this
directory, like a VHS cassette but for Knot's live preview: `record.py` starts
`knot-lsp` as an editor would, opens Knot's preview, then types and saves in a
copy of `fixture/` with the messages VS Code sends. A headless browser records
`stage.html`, which shows the source next to the preview. Nothing is read from
or written to your own projects.

The scenario: a prose edit (instant, from the cache), a regression line added
to the figure (amber while typing, orange once saved, the previous figure kept
veiled until the new one), a typo that raises an R error in place (later R
chunks suspended, Python still runs), and its fix.

## Recording

From the repository root:

```bash
cargo build --release
python3 -m venv demo/.venv
demo/.venv/bin/pip install playwright
demo/.venv/bin/playwright install chromium
demo/.venv/bin/python demo/record.py
```

Requirements: R with `ggplot2`, `svglite`, `jsonlite` and `digest`; Python 3;
`ffmpeg`; Tinymist (the binary bundled with the VS Code extension is used when
present, otherwise set `TINYMIST`). Typst downloads `codly` on first use.

Regenerating is a manual step, deliberately kept out of CI: it needs R, a
browser and Tinymist at once, and the result is reviewed by eye.
