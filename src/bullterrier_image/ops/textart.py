"""Text art: a picture redrawn as a grid of characters, each chosen by how much ink it puts down.

The one algorithm behind every effect that turns a picture (or part of one) into text. Callers decide what the text
looks like: the character palette, its size, the colours and how much of the picture shows through. Glyph shapes and
the font are `ops.glyphs`'s business.
"""

import numpy as np

from bullterrier_image.ops.color import ink_amount, luminance, vivid
from bullterrier_image.ops.glyphs import glyph_atlas, paint


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
    ink_floor: float,
    paper: np.ndarray,
    paper_threshold: float,
    gamma: float,
    photo: float,
) -> np.ndarray:
    """Redraw `rgb` (float32 (h, w, 3), 0..1) as characters; returns a new float32 image of the same shape.

    cell: character width in px (the height follows from the font, see `glyphs.cell_size`).
    chars: the palette, in any order, at least two distinct characters; each cell gets the one whose ink matches the
        cell's brightness: the densest for bright cells on dark paper, for dark cells on light paper (`ink_amount`).
    ink: the ink colour, float32 RGB 0..1; None colours each character from the picture under it, brightened so
        thin strokes stay readable (`ops.color.vivid`): scaled until its strongest channel is full, by at most
        1 / `ink_floor` (0 < ink_floor <= 1; ignored when `ink` is given).
    paper: the paper colour, float32 RGB 0..1; it is dark when its Rec. 709 luminance is below `paper_threshold`
        (0..1).
    gamma: exponent on the ink amount, > 0; above 1 puts down less ink in the midtones, below 1 more.
    photo: 0..1, how much of the picture shows through the paper behind the characters (0 = flat paper colour).
    A cell cut off by the picture's right or bottom edge is measured as if the border pixels continued.
    """
    atlas = glyph_atlas(chars, cell)  # (n_glyphs, cell_h, cell_w) ink coverage, sorted light -> dense
    n, cell_h, cell_w = atlas.shape
    height, width = rgb.shape[:2]
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    # the border pixels are repeated: a row or column past the picture reads the last one, so a partial edge cell
    # averages real picture, not black
    padded = rgb[
        np.ix_(np.minimum(np.arange(rows * cell_h), height - 1), np.minimum(np.arange(cols * cell_w), width - 1))
    ]

    # The colour and brightness of each character cell is the average of the pixels it covers.
    cells = padded.reshape(rows, cell_h, cols, cell_w, 3).mean(axis=(1, 3))
    amount = np.clip(ink_amount(luminance(cells), paper, threshold=paper_threshold), 0.0, 1.0) ** gamma
    glyph = np.rint(amount * (n - 1)).astype(int)

    coverage = paint(glyph, atlas)[..., None]
    text = np.repeat(np.repeat(vivid(cells, floor=ink_floor), cell_h, axis=0), cell_w, axis=1) if ink is None else ink
    base = paper * (1.0 - photo) + padded * photo
    out = base * (1.0 - coverage) + text * coverage
    return out[:height, :width].astype(np.float32)
