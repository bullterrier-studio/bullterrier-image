"""ASCII art: the picture redrawn as a grid of text characters."""

import numpy as np

from photo_fx.ops.glyphs import cell_size, glyph_atlas, paint

from ._common import Color, ink_amount, luminance, parse_color, vivid
from ._registry import effect

# Other effects and tests still import these names from here.
_glyph_atlas = glyph_atlas
__all__ = ["_glyph_atlas", "ascii", "cell_size"]


@effect(category="text")
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
    atlas = glyph_atlas(chars, cell, font)  # (n_glyphs, cell_h, cell_w) ink coverage, sorted light -> dense
    n, cell_h, cell_w = atlas.shape
    height, width = rgb.shape[:2]
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    padded = np.pad(rgb, ((0, rows * cell_h - height), (0, cols * cell_w - width), (0, 0)), mode="edge")

    # The colour and brightness of each character cell is the average of the pixels it covers.
    cells = padded.reshape(rows, cell_h, cols, cell_w, 3).mean(axis=(1, 3))
    ink = np.clip(ink_amount(luminance(cells), bg_color), 0.0, 1.0) ** gamma
    glyph = np.rint(ink * (n - 1)).astype(int)

    coverage = paint(glyph, atlas)[..., None]
    if fg is None:
        text = np.repeat(np.repeat(vivid(cells), cell_h, axis=0), cell_w, axis=1)
    else:
        text = parse_color(fg)
    base = bg_color * (1.0 - photo) + padded * photo
    out = base * (1.0 - coverage) + text * coverage
    return out[:height, :width].astype(np.float32)
