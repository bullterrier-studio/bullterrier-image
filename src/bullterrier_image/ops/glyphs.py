"""Text characters as pixels: the shared machinery behind every effect that draws with glyphs.

The module promises three things. `cell_size` says how big one character cell is, so effects can lay a grid over a
picture. `glyph_atlas` gives the ink coverage of each distinct character of a palette, ordered from least to most ink,
so a brightness maps straight to an atlas index. `paint` turns a grid of such indices back into a coverage picture.

Every glyph is drawn with the one font in `FONT`, never a system font, so a look is the same on every platform
(within float tolerance). It is JetBrains Mono Regular (SIL OFL 1.1, `LICENSE-FONT`), monospaced, with Latin letters
and the block elements (`█▓▒░▀▄`). A character it lacks (braille, for one) shows as its missing-glyph box.
"""

from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

FONT = Path(__file__).parent.parent / "fonts" / "JetBrainsMono-Regular.ttf"
"""The bundled monospaced font every glyph is drawn with."""
_FULL_INK = 255  # constant: full scale of an 8-bit grey canvas, where a glyph is drawn white on black
# One glyph per cell, so no ligatures or kerning are wanted. Pillow's basic layout engine measures a glyph the same on
# every build; its optional Raqm engine keeps fractional advances where the basic one rounds, which would shift glyphs
# by a pixel from machine to machine.
_LAYOUT = ImageFont.Layout.BASIC


def cell_size(cell: int) -> tuple[int, int]:
    """(width, height) in px of one character cell when characters are `cell` px wide.

    `cell` = character width in px (a positive int). The width is always exactly `cell`; the height comes from the font
    and grows with `cell`.
    This is the grid every glyph drawn by `glyph_atlas` and `paint` lives on, so callers can lay out whole characters.
    """
    ascent, descent = _load_font(cell).getmetrics()
    return cell, ascent + descent


def glyph_atlas(chars: str, cell: int) -> np.ndarray:
    """Ink coverage of each distinct character of `chars`, ordered from least to most ink.

    `chars` = the palette (at least two distinct characters), in any order; repeated characters count once. `cell` =
    character width in px. Returns a float32 array of shape (n_glyphs, cell_h, cell_w) with coverage in 0..1 (0 = no
    ink, 1 = fully inked), where (cell_w, cell_h) == `cell_size(cell)`. Index 0 is the lightest glyph, the last index
    the densest. The result is cached and shared between callers, so it is read-only: copy it before editing.
    """
    return _atlas(chars, cell)


@lru_cache(maxsize=16)  # constant: a cache size, which never changes a result
def _atlas(chars: str, cell: int) -> np.ndarray:
    """`glyph_atlas`, cached by its arguments."""
    glyph_font = _load_font(cell)
    ascent, descent = glyph_font.getmetrics()
    cell_w, cell_h = cell, ascent + descent
    bitmaps = []
    for ch in dict.fromkeys(chars):  # unique, order kept
        canvas = Image.new("L", (cell_w, cell_h), 0)  # constant: 8-bit grey, no ink
        # The glyph is centred in its cell: the middle of its advance width over the middle of the cell, and the middle
        # of its line (halfway between ascent and descent) over the middle of the cell, each in whole px with a half
        # rounded up, so its pixels are those Pillow's own centring anchor ("mm") would draw. That anchor is not used:
        # it is a library option, and measuring the font and offsetting says what it does.
        left = cell_w / 2 - (glyph_font.getlength(ch) + 1) // 2
        top = cell_h / 2 + (ascent - descent + 1) // 2 - ascent
        ImageDraw.Draw(canvas).text((left, top), ch, fill=_FULL_INK, font=glyph_font)
        bitmaps.append(np.asarray(canvas, dtype=np.float32) / float(_FULL_INK))
    bitmaps.sort(key=lambda b: float(b.mean()))
    atlas = np.stack(bitmaps)
    atlas.flags.writeable = False  # shared through the cache: one caller's edit must not reach the others
    return atlas


def paint(indices: np.ndarray, atlas: np.ndarray) -> np.ndarray:
    """Draw a grid of glyphs: cell (r, c) of the result holds the atlas glyph `indices[r, c]`.

    `indices` = (rows, cols) integers, each in 0..n_glyphs-1.
    `atlas` = (n_glyphs, cell_h, cell_w) coverage as returned by `glyph_atlas`.
    Returns a float32 (rows * cell_h, cols * cell_w) coverage array in 0..1.
    """
    rows, cols = indices.shape
    _, cell_h, cell_w = atlas.shape
    return atlas[indices].transpose(0, 2, 1, 3).reshape(rows * cell_h, cols * cell_w)


@lru_cache(maxsize=16)  # constant: a cache size, which never changes a result
def _load_font(cell: int) -> ImageFont.FreeTypeFont:
    """`FONT` sized so an 'M' is `cell` px wide."""
    reference = ImageFont.truetype(FONT, 100, layout_engine=_LAYOUT)  # constant: a measuring size; it cancels out below
    em_per_advance = 100.0 / reference.getlength("M")  # constant: the same measuring size
    # constant: FreeType's smallest legible size
    return ImageFont.truetype(FONT, max(4, round(cell * em_per_advance)), layout_engine=_LAYOUT)
