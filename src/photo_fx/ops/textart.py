"""Text art: a picture redrawn as a grid of characters, each chosen by how much ink it puts down.

The one algorithm behind every effect that turns a picture (or part of one) into text. Callers decide what the text
looks like: the character palette, its size, the colours and how much of the picture shows through. Glyph shapes and
fonts are `ops.glyphs`'s business.
"""

import numpy as np

from photo_fx.ops.color import ink_amount, luminance, vivid
from photo_fx.ops.glyphs import glyph_atlas, paint


def check_palette(chars: str) -> str:
    """`chars` unchanged if it can be a `text_art` palette: at least two distinct characters, so that brightness has
    something to choose between. Raises ValueError naming the palette otherwise; parameter models use it to refuse a bad
    palette when it is written, not when it is drawn."""
    if len(set(chars)) < 2:
        raise ValueError(f"a palette needs at least two different characters, got {chars!r}")
    return chars


def text_art(
    rgb: np.ndarray,
    *,
    cell: int,
    chars: str,
    ink: np.ndarray | None,
    paper: np.ndarray,
    gamma: float,
    photo: float,
    font: str | None,
) -> np.ndarray:
    """Redraw `rgb` (float32 (h, w, 3), 0..1) as characters; returns a new float32 image of the same shape.

    cell: character width in px (the height follows from the font, see `glyphs.cell_size`).
    chars: the palette, in any order, at least two distinct characters; each cell gets the one whose ink matches the
        cell's brightness: the densest for bright cells on dark paper, for dark cells on light paper (`ink_amount`;
        paper is dark below mid grey, Rec. 709 luminance 0.5).
    ink: the ink colour, float32 RGB 0..1; None colours each character from the picture under it, brightened so
        thin strokes stay readable (`ops.color.vivid`). paper: the paper colour, float32 RGB 0..1.
    gamma: exponent on the ink amount, > 0; above 1 puts down less ink in the midtones, below 1 more.
    photo: 0..1, how much of the picture shows through the paper behind the characters (0 = flat paper colour).
    font: path to a monospaced .ttf/.ttc, or None for the first system font found.
    A cell cut off by the picture's right or bottom edge is measured as if the border pixels continued.
    Raises ValueError if `chars` has fewer than two distinct characters.
    """
    atlas = glyph_atlas(chars, cell, font)  # (n_glyphs, cell_h, cell_w) ink coverage, sorted light -> dense
    n, cell_h, cell_w = atlas.shape
    height, width = rgb.shape[:2]
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    # constant: repeat the border pixels, so a partial edge cell averages real picture, not black
    padded = np.pad(rgb, ((0, rows * cell_h - height), (0, cols * cell_w - width), (0, 0)), mode="edge")

    # The colour and brightness of each character cell is the average of the pixels it covers.
    # constant: three colour channels, red, green and blue
    cells = padded.reshape(rows, cell_h, cols, cell_w, 3).mean(axis=(1, 3))
    amount = np.clip(ink_amount(luminance(cells), paper), 0.0, 1.0) ** gamma
    glyph = np.rint(amount * (n - 1)).astype(int)

    coverage = paint(glyph, atlas)[..., None]
    text = np.repeat(np.repeat(vivid(cells), cell_h, axis=0), cell_w, axis=1) if ink is None else ink
    base = paper * (1.0 - photo) + padded * photo
    out = base * (1.0 - coverage) + text * coverage
    return out[:height, :width].astype(np.float32)
