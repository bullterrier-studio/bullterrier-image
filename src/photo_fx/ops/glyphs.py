"""Text characters as pixels: the shared machinery behind every effect that draws with glyphs.

The module promises three things. `cell_size` says how big one character cell is, so effects can lay a grid over a
picture. `glyph_atlas` gives the ink coverage of each distinct character of a palette, ordered from least to most ink,
so a brightness maps straight to an atlas index. `paint` turns a grid of such indices back into a coverage picture.
Fonts, their sizing and the rendering cache are internal: callers only ever see cells, atlases and coverage arrays.

Which font draws, when the caller names none: the first file that exists among those listed in the `PHOTO_FX_FONTS`
environment variable (paths separated by `os.pathsep`; set but empty lists none), or, when it is unset, among the usual
monospaced system fonts (Menlo, Monaco, Courier on macOS; DejaVu Sans Mono on Linux; Consolas on Windows). A host with
none of them (a slim Linux container) still works: Pillow's own built-in font draws, sized for cells `cell` px
wide. That font is not monospaced, and it lacks the block elements, so FULL BLOCK and DARK, MEDIUM and LIGHT SHADE
(`█▓▒░`) are then drawn as a cell inked evenly at 100, 75, 50 and 25 %. Any other character a font lacks shows as the
font's missing-glyph box.
"""

import os
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

FONTS_VARIABLE = "PHOTO_FX_FONTS"
# Tried in order when FONTS_VARIABLE is unset; the first that exists wins. All are monospaced.
_FONTS = [
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Monaco.ttf",
    "/System/Library/Fonts/Courier.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "C:/Windows/Fonts/consola.ttf",
]
# The Unicode block elements U+2588 FULL BLOCK and U+2593..2591 DARK, MEDIUM, LIGHT SHADE, drawn as such when the
# font has no glyph for them
# constant: the share of the cell each one's Unicode name gives it
_BLOCK_INK = {"█": 1.0, "▓": 0.75, "▒": 0.5, "░": 0.25}
_NONCHARACTER = "\uffff"  # never assigned (Unicode noncharacter), so every font draws its missing-glyph box for it


def cell_size(cell: int, font: str | None = None) -> tuple[int, int]:
    """(width, height) in px of one character cell when characters are `cell` px wide.

    `cell` = character width in px (a positive int). `font` = path to a monospaced .ttf/.ttc, or None for the font
    chosen as the module documentation says. The width is always exactly `cell`; the height comes from the font and
    grows with `cell`.
    This is the grid every glyph drawn by `glyph_atlas` and `paint` lives on, so callers can lay out whole characters.
    Raises OSError if `font` is given but cannot be opened as a font file.
    """
    ascent, descent = _load_font(cell, _font_file(font)).getmetrics()
    return cell, ascent + descent


def glyph_atlas(chars: str, cell: int, font: str | None = None) -> np.ndarray:
    """Ink coverage of each distinct character of `chars`, ordered from least to most ink.

    `chars` = the palette, in any order; repeated characters count once. `cell` = character width in px and `font`
    as in `cell_size`. Returns a float32 array of shape (n_glyphs, cell_h, cell_w) with coverage in 0..1 (0 = no ink,
    1 = fully inked), where (cell_w, cell_h) == `cell_size(cell, font)`. Index 0 is the lightest glyph, the last index
    the densest. The result is cached and shared between callers, so it is read-only: copy it before editing.
    Raises ValueError if `chars` has fewer than two distinct characters (there is nothing to choose between) and
    OSError if `font` is given but cannot be opened as a font file.
    """
    return _atlas(chars, cell, _font_file(font))


@lru_cache(maxsize=16)  # constant: a cache size, which never changes a result
def _atlas(chars: str, cell: int, font: str | None) -> np.ndarray:
    """`glyph_atlas` for a font file already chosen (None: Pillow's built-in font), cached by that file."""
    glyph_font = _load_font(cell, font)
    ascent, descent = glyph_font.getmetrics()
    cell_w, cell_h = cell, ascent + descent
    bitmaps = []
    for ch in dict.fromkeys(chars):  # unique, order kept
        if ch in _BLOCK_INK and _lacks(glyph_font, ch):
            bitmaps.append(np.full((cell_h, cell_w), _BLOCK_INK[ch], dtype=np.float32))
            continue
        canvas = Image.new("L", (cell_w, cell_h), 0)  # constant: 8-bit grey, no ink
        # constant: full 8-bit ink, the glyph centred in its cell
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
    if atlas.ndim != 3:  # constant: glyphs, rows, columns
        raise ValueError(f"paint needs an atlas of shape (n_glyphs, cell_h, cell_w), got {atlas.ndim}-D")
    if indices.size and (indices.min() < 0 or indices.max() >= len(atlas)):
        raise IndexError(f"glyph index outside the atlas of {len(atlas)} glyphs")
    rows, cols = indices.shape
    _, cell_h, cell_w = atlas.shape
    return atlas[indices].transpose(0, 2, 1, 3).reshape(rows * cell_h, cols * cell_w)


def _font_file(font: str | None) -> str | None:
    """The font file to draw with: `font` itself, else the first listed file that exists, else None (Pillow's own)."""
    if font is not None:
        return font
    listed = os.environ.get(FONTS_VARIABLE)
    candidates = _FONTS if listed is None else [p for p in listed.split(os.pathsep) if p]
    return next((p for p in candidates if Path(p).is_file()), None)


def _lacks(font: ImageFont.FreeTypeFont, ch: str) -> bool:
    """True when `font` has no glyph for `ch`: it draws exactly what it draws for a character no font has."""
    return font.getbbox(ch) == font.getbbox(_NONCHARACTER) and bytes(font.getmask(ch)) == bytes(
        font.getmask(_NONCHARACTER)
    )


@lru_cache(maxsize=16)  # constant: a cache size, which never changes a result
def _load_font(cell: int, font_path: str | None) -> ImageFont.FreeTypeFont:
    """The font of `font_path` sized so an 'M' is `cell` px wide; None: Pillow's built-in font with an em of
    `cell` / 0.6 px (a typical monospaced font advances 0.6 em per character)."""
    if font_path is None:
        # constant: a monospaced advance is about 0.6 em, so this em makes one character `cell` px wide
        default = ImageFont.load_default(size=round(cell / 0.6))
        # Pillow falls back to a fixed-size bitmap font, which cannot be measured or scaled, only without FreeType
        if not isinstance(default, ImageFont.FreeTypeFont):
            # Imported here: the error types are the one thing of photo_fx's that ops may use (pyproject's contracts)
            from photo_fx.core.errors import BackendUnavailableError

            raise BackendUnavailableError(
                f"no font file was found and this Pillow has no FreeType to draw its own: list a monospaced .ttf or "
                f".ttc in {FONTS_VARIABLE} (paths separated by {os.pathsep!r}), pass `font`, or install a Pillow "
                "built with FreeType"
            )
        return default
    reference = ImageFont.truetype(font_path, 100)  # constant: a measuring size; it cancels out below
    em_per_advance = 100.0 / reference.getlength("M")  # constant: the same measuring size
    # constant: FreeType's smallest legible size
    return ImageFont.truetype(font_path, max(4, round(cell * em_per_advance)))
