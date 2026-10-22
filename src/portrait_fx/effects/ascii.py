"""ASCII art: the picture redrawn as a grid of text characters."""

from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from ._common import Color, ink_amount, luminance, parse_color, vivid
from ._registry import effect

# Tried in order; the first that exists wins. All are monospaced.
_FONTS = [
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Monaco.ttf",
    "/System/Library/Fonts/Courier.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "C:/Windows/Fonts/consola.ttf",
]


@effect
def ascii(
    rgb: np.ndarray,
    cell: int = 10,
    chars: str = " .:-=+*#%@",
    fg: Color | None = None,
    bg: Color = "#000000",
    gamma: float = 1.0,
    photo: float = 0.0,
    font: str | None = None,
) -> np.ndarray:
    """Redraw the picture as text. `cell` is the character width in px (smaller = finer detail).

    `chars` is the palette in any order; it is sorted by how much ink each glyph actually puts on the page, so
    '@' is used for the brightest areas on a dark `bg` and the darkest on a light one. Try " .:-=+*#%@", " .oO0@",
    "01", or block characters like " ░▒▓█" (the font must have them). `fg` = one colour for all text; leave it
    out to colour each character from the photo. `gamma` > 1 darkens the midtones.
    `photo` (0..1) is how much of the original picture shows through behind the characters: 0 = text on a flat `bg`
    (fine detail is lost to the character grid), 1 = text laid over the untouched photo, so every detail stays visible.
    `font` = path to a monospaced .ttf/.ttc if you do not want the system one.
    """
    bg_color = parse_color(bg)
    atlas = _glyph_atlas(chars, cell, font)  # (n_glyphs, cell_h, cell_w) ink coverage, sorted light -> dense
    n, cell_h, cell_w = atlas.shape
    height, width = rgb.shape[:2]
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    padded = np.pad(rgb, ((0, rows * cell_h - height), (0, cols * cell_w - width), (0, 0)), mode="edge")

    # The colour and brightness of each character cell is the average of the pixels it covers.
    cells = padded.reshape(rows, cell_h, cols, cell_w, 3).mean(axis=(1, 3))
    ink = np.clip(ink_amount(luminance(cells), bg_color), 0.0, 1.0) ** gamma
    glyph = np.rint(ink * (n - 1)).astype(int)

    coverage = atlas[glyph].transpose(0, 2, 1, 3).reshape(rows * cell_h, cols * cell_w, 1)
    if fg is None:
        text = np.repeat(np.repeat(vivid(cells), cell_h, axis=0), cell_w, axis=1)
    else:
        text = parse_color(fg)
    base = bg_color * (1.0 - photo) + padded * photo
    out = base * (1.0 - coverage) + text * coverage
    return out[:height, :width].astype(np.float32)


@lru_cache(maxsize=16)
def _glyph_atlas(chars: str, cell: int, font_path: str | None) -> np.ndarray:
    """Render each distinct character once into a (cell_h, cell_w) coverage bitmap, ordered from least to most ink."""
    glyph_font = _load_font(cell, font_path)
    ascent, descent = glyph_font.getmetrics()
    cell_w, cell_h = cell, ascent + descent
    bitmaps = []
    for ch in dict.fromkeys(chars):  # unique, order kept
        canvas = Image.new("L", (cell_w, cell_h), 0)
        ImageDraw.Draw(canvas).text((cell_w / 2, cell_h / 2), ch, fill=255, font=glyph_font, anchor="mm")
        bitmaps.append(np.asarray(canvas, dtype=np.float32) / 255.0)
    if len(bitmaps) < 2:
        raise ValueError("ascii needs at least two different characters in `chars`")
    bitmaps.sort(key=lambda b: float(b.mean()))
    return np.stack(bitmaps)


def _load_font(cell: int, font_path: str | None) -> ImageFont.FreeTypeFont:
    """A monospaced font sized so one character is `cell` px wide."""
    path = font_path or next((p for p in _FONTS if Path(p).exists()), None)
    if path is None:
        return ImageFont.load_default(size=round(cell / 0.6))
    reference = ImageFont.truetype(path, 100)
    em_per_advance = 100.0 / reference.getlength("M")
    return ImageFont.truetype(path, max(4, round(cell * em_per_advance)))
