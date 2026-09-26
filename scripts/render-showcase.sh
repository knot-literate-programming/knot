#!/usr/bin/env bash
# Regenerate the Anscombe showcase (README and book) from examples/anscombe:
# docs/book/src/assets/anscombe.pdf and the two excerpts in docs/book/src/images/.
# Requires a release build of knot, typst, the Anscombe dependencies (see its
# README) and Python with Pillow. Run before a release, after Knot changes.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
knot="${KNOT:-$root/target/release/knot}"
work="$(mktemp -d)"
trap 'rm -r "$work"' EXIT

cp -R "$root/examples/anscombe/." "$work/"
(cd "$work" && "$knot" build --strict --no-snapshots)
cp "$work/main.pdf" "$root/docs/book/src/assets/anscombe.pdf"
typst compile "$work/main.typ" "$work/page-{p}.png" --ppi 130 --pages 4,5

python3 - "$work" "$root/docs/book/src/images" <<'PY'
import sys
from PIL import Image

work, images = sys.argv[1:]

def ink_rows(image):
    """Rows of the text column holding dark pixels."""
    width, height = image.size
    pixels = image.load()
    return [y for y in range(height)
            if any(sum(pixels[x, y]) < 600 for x in range(int(0.1 * width), int(0.9 * width), 2))]

def blocks(rows, gap=25):
    """Vertical blocks of ink separated by more than `gap` blank rows."""
    found, start, previous = [], rows[0], rows[0]
    for y in rows[1:]:
        if y - previous > gap:
            found.append((start, previous))
            start = y
        previous = y
    return found + [(start, previous)]

def save(page, first, last, name):
    image = Image.open(f"{work}/page-{page}.png").convert("RGB")
    width = image.size[0]
    image.crop((int(0.1 * width), first - 12, int(0.9 * width), last + 12)).save(
        f"{images}/{name}", optimize=True)

# Page 4: below the chapter title, down to the end of the figure; the
# commentary after it follows the largest gap of the page.
rows = ink_rows(Image.open(f"{work}/page-4.png").convert("RGB"))
start = blocks(rows, gap=12)[1][0]
gaps = [(b - a, a) for a, b in zip(rows, rows[1:]) if a > start]
save(4, start, max(gaps)[1], "anscombe-mixed.png")
# Page 5: the validation section, without the bibliography.
page5 = blocks(ink_rows(Image.open(f"{work}/page-5.png").convert("RGB")))
save(5, page5[0][0], page5[0][1], "anscombe-validation.png")
PY
echo "Updated the Anscombe PDF and excerpts; check them before committing."
