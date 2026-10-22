"""Text characters as pixels: the shared machinery behind every effect that draws with glyphs.

The module promises three things. `cell_size` says how big one character cell is, so effects can lay a grid over a
picture. `glyph_atlas` gives the ink coverage of each distinct character of a palette, ordered from least to most ink,
so a brightness maps straight to an atlas index. `paint` turns a grid of such indices back into a coverage picture.
Fonts, their sizing and the rendering cache are internal: callers only ever see cells, atlases and coverage arrays.
"""

from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

# Tried in order; the first that exists wins. All are monospaced.
_FONTS = [
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Monaco.ttf",
    "/System/Library/Fonts/Courier.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "C:/Windows/Fonts/consola.ttf",
]


def cell_size(cell: int, font: str | None = None) -> tuple[int, int]:
    """(width, height) in px of one character cell when characters are `cell` px wide.

    `cell` = character width in px (a positive int). `font` = path to a monospaced .ttf/.ttc, or None for the first
    system font found. The width is always exactly `cell`; the height comes from the font and grows with `cell`.
    This is the grid every glyph drawn by `glyph_atlas` and `paint` lives on, so callers can lay out whole characters.
    Raises OSError if `font` is given but cannot be opened as a font file.
    """
    ascent, descent = _load_font(cell, font).getmetrics()
    return cell, ascent + descent


@lru_cache(maxsize=16)
def glyph_atlas(chars: str, cell: int, font: str | None = None) -> np.ndarray:
    """Ink coverage of each distinct character of `chars`, ordered from least to most ink.

    `chars` = the palette, in any order; repeated characters count once. `cell` = character width in px and `font`
    as in `cell_size`. Returns a float32 array of shape (n_glyphs, cell_h, cell_w) with coverage in 0..1 (0 = no ink,
    1 = fully inked), where (cell_w, cell_h) == `cell_size(cell, font)`. Index 0 is the lightest glyph, the last index
    the densest. The result is cached and shared between callers, so it is read-only: copy it before editing.
    Raises ValueError if `chars` has fewer than two distinct characters (there is nothing to choose between) and
    OSError if `font` is given but cannot be opened as a font file.
    """
    glyph_font = _load_font(cell, font)
    cell_w, cell_h = cell_size(cell, font)
    bitmaps = []
    for ch in dict.fromkeys(chars):  # unique, order kept
        canvas = Image.new("L", (cell_w, cell_h), 0)
        ImageDraw.Draw(canvas).text((cell_w / 2, cell_h / 2), ch, fill=255, font=glyph_font, anchor="mm")
        bitmaps.append(np.asarray(canvas, dtype=np.float32) / 255.0)
    if len(bitmaps) < 2:
        raise ValueError("glyph_atlas needs at least two different characters in `chars`")
    bitmaps.sort(key=lambda b: float(b.mean()))
    atlas = np.stack(bitmaps)
    atlas.flags.writeable = False  # shared through the cache: one caller's edit must not reach the others
    return atlas


def paint(indices: np.ndarray, atlas: np.ndarray) -> np.ndarray:
    """Draw a grid of glyphs: cell (r, c) of the result holds the atlas glyph `indices[r, c]`.

    `indices` = (rows, cols) integers, each in 0..n_glyphs-1 (negative values are rejected, not wrapped).
    `atlas` = (n_glyphs, cell_h, cell_w) coverage as returned by `glyph_atlas`.
    Returns a float32 (rows * cell_h, cols * cell_w) coverage array in 0..1.
    Raises ValueError if `indices` is not a 2-D integer array or `atlas` is not 3-D, and IndexError if any index lies
    outside the atlas.
    """
    if indices.ndim != 2:
        raise ValueError(f"paint needs a 2-D grid of indices, got {indices.ndim}-D")
    if not np.issubdtype(indices.dtype, np.integer):
        raise ValueError(f"paint needs integer indices, got {indices.dtype}")
    if atlas.ndim != 3:
        raise ValueError(f"paint needs an atlas of shape (n_glyphs, cell_h, cell_w), got {atlas.ndim}-D")
    if indices.size and (indices.min() < 0 or indices.max() >= len(atlas)):
        raise IndexError(f"glyph index outside the atlas of {len(atlas)} glyphs")
    rows, cols = indices.shape
    _, cell_h, cell_w = atlas.shape
    return atlas[indices].transpose(0, 2, 1, 3).reshape(rows * cell_h, cols * cell_w)


@lru_cache(maxsize=16)
def _load_font(cell: int, font_path: str | None) -> ImageFont.FreeTypeFont:
    """A monospaced font sized so one character is `cell` px wide."""
    path = font_path or next((p for p in _FONTS if Path(p).exists()), None)
    if path is None:
        default = ImageFont.load_default(size=round(cell / 0.6))
        # Pillow falls back to a fixed-size bitmap font, which cannot be measured or scaled, only without FreeType
        if not isinstance(default, ImageFont.FreeTypeFont):
            raise RuntimeError("no system font was found and this Pillow has no FreeType to draw its own; pass `font`")
        return default
    reference = ImageFont.truetype(path, 100)
    em_per_advance = 100.0 / reference.getlength("M")
    return ImageFont.truetype(path, max(4, round(cell * em_per_advance)))
