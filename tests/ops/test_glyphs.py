from pathlib import Path

import numpy as np
import pytest
from PIL import Image, ImageDraw, ImageFont

from photo_fx.ops.glyphs import cell_size, glyph_atlas, paint

CELLS = [5, 8, 13, 20]


def ink(atlas: np.ndarray) -> np.ndarray:
    return atlas.sum(axis=(1, 2))


def test_cell_width_is_the_requested_width_and_height_grows_with_it():
    sizes = [cell_size(c) for c in CELLS]
    assert [w for w, _ in sizes] == CELLS
    heights = [h for _, h in sizes]
    assert all(h > 0 for h in heights)
    assert heights == sorted(set(heights))  # strictly increasing


@pytest.mark.parametrize("cell", [5, 8, 13])
def test_atlas_shape_matches_cell_size(cell):
    width, height = cell_size(cell)
    atlas = glyph_atlas("a. #", cell)
    assert atlas.shape == (4, height, width)
    assert atlas.dtype == np.float32
    assert 0.0 <= atlas.min()
    assert atlas.max() <= 1.0


def test_atlas_is_ordered_from_least_to_most_ink():
    atlas = glyph_atlas("@.:#o ", 10)  # given deliberately out of order
    totals = ink(atlas)
    assert atlas.shape[0] == 6
    assert np.all(np.diff(totals) >= 0)
    assert totals[0] < totals[-1]


def test_duplicate_characters_collapse():
    with_repeats = glyph_atlas("aabba .", 8)
    assert with_repeats.shape[0] == 4
    np.testing.assert_array_equal(with_repeats, glyph_atlas("ab .", 8))


@pytest.mark.parametrize("chars", ["", "x", "xxxx"])
def test_fewer_than_two_distinct_characters_raises(chars):
    with pytest.raises(ValueError, match="at least two"):
        glyph_atlas(chars, 8)


def test_space_has_no_ink_and_the_full_block_has_the_most():
    atlas = glyph_atlas(" .:#█", 10)
    totals = ink(atlas)
    assert totals[0] == 0.0  # the space sorts first
    assert totals[-1] == totals.max()
    assert totals[-1] > totals[:-1].max()
    block_alone = glyph_atlas(" █", 10)[1]  # the block as rendered with only a space for company
    np.testing.assert_array_equal(atlas[-1], block_alone)
    assert block_alone.mean() > 0.5  # it really is a filled block, not an empty glyph


def test_atlas_is_cached_and_cannot_be_modified_in_place():
    first = glyph_atlas(" .:#", 9)
    assert glyph_atlas(" .:#", 9) is first
    assert not first.flags.writeable
    with pytest.raises(ValueError):
        first[0, 0, 0] = 1.0


def test_paint_places_glyph_i_in_cell_r_c():
    atlas = glyph_atlas(" .:#█", 6)
    height, width = atlas.shape[1:]
    indices = np.array([[0, 3, 4], [2, 1, 3]])
    out = paint(indices, atlas)
    assert out.shape == (2 * height, 3 * width)
    assert out.dtype == np.float32
    for r in range(2):
        for c in range(3):
            np.testing.assert_array_equal(
                out[r * height : (r + 1) * height, c * width : (c + 1) * width], atlas[indices[r, c]]
            )


@pytest.mark.parametrize(("rows", "cols"), [(1, 5), (4, 1), (3, 2)])
def test_paint_output_shape_follows_rows_and_columns(rows, cols):
    atlas = glyph_atlas(" .#", 7)
    height, width = atlas.shape[1:]
    assert height != width
    assert paint(np.ones((rows, cols), dtype=int), atlas).shape == (rows * height, cols * width)


def test_paint_of_an_empty_grid_is_empty():
    atlas = glyph_atlas(" .#", 7)
    assert paint(np.zeros((0, 3), dtype=int), atlas).shape == (0, 3 * atlas.shape[2])


@pytest.mark.parametrize("bad", [3, 8, -1])
def test_paint_rejects_indices_outside_the_atlas(bad):
    atlas = glyph_atlas(" .#", 7)  # 3 glyphs: valid indices are 0..2, and -1 must not wrap to the last glyph
    with pytest.raises(IndexError):
        paint(np.array([[0, bad]]), atlas)


@pytest.mark.parametrize("shape", [(3,), (1, 2, 3)])
def test_paint_rejects_indices_that_are_not_a_grid(shape):
    atlas = glyph_atlas(" .#", 7)
    with pytest.raises(ValueError, match="2-D"):
        paint(np.zeros(shape, dtype=int), atlas)


def test_paint_rejects_non_integer_indices():
    atlas = glyph_atlas(" .#", 7)
    with pytest.raises(ValueError, match="integer"):
        paint(np.array([[0.0, 1.0]]), atlas)


@pytest.mark.parametrize("shape", [(7, 4), (2, 7, 4, 1)])
def test_paint_rejects_an_atlas_that_is_not_a_stack_of_glyphs(shape):
    with pytest.raises(ValueError, match="atlas"):
        paint(np.zeros((1, 1), dtype=int), np.zeros(shape, dtype=np.float32))


# --- fonts and glyph geometry -----------------------------------------------------------------------------------
# These hold for any sane monospaced font, so they run against whatever monospaced fonts the host has installed.

_FONT_DIRS = ["/System/Library/Fonts", "/Library/Fonts", "~/Library/Fonts", "/usr/share/fonts", "C:/Windows/Fonts"]


def _is_monospaced_text_font(path: Path) -> bool:
    """Monospaced, and with real Latin letters (symbol and braille fonts draw every letter as the same blank box)."""
    try:
        font = ImageFont.truetype(str(path), 100)
        monospaced = len({round(font.getlength(ch), 1) for ch in "Mi.W"}) == 1
        letters = {(font.getmask(ch).size, bytes(font.getmask(ch))) for ch in "MHIO"}
        return monospaced and len(letters) == 4
    except OSError:
        return False


def _host_monospaced_fonts(limit: int = 6) -> list[str]:
    candidates: list[Path] = []
    for folder in _FONT_DIRS:
        root = Path(folder).expanduser()
        if root.is_dir():
            candidates += sorted(p for p in root.rglob("*") if p.suffix.lower() in {".ttf", ".ttc"})
    found: list[str] = []
    for path in candidates:
        if path.name.startswith("SFNS"):
            continue  # Apple's variable system fonts report line metrics that disagree with where Pillow anchors text
        if str(path) not in found and path.exists() and _is_monospaced_text_font(path):
            found.append(str(path))
        if len(found) == limit:
            break
    return found


HOST_FONTS = _host_monospaced_fonts()
no_font = pytest.param(None, marks=pytest.mark.skip(reason="no monospaced font file found on this host"))
host_fonts = pytest.mark.parametrize("font", HOST_FONTS or [no_font])


def _real_font_sizes(font: str, cell: int) -> list[ImageFont.FreeTypeFont]:
    """Every font size Pillow itself says has an 'M' `cell` px wide (+-0.5): the independent oracle for the geometry."""
    sizes = [ImageFont.truetype(font, size) for size in range(4, 200)]
    return [f for f in sizes if abs(f.getlength("M") - cell) <= 0.5]


def test_a_font_that_does_not_exist_raises_oserror():
    missing = "/no/such/dir/NoSuchFont.ttf"
    with pytest.raises(OSError):
        cell_size(8, missing)
    with pytest.raises(OSError):
        glyph_atlas(" .#", 8, missing)


@pytest.mark.skipif(len(HOST_FONTS) < 2, reason="needs two different monospaced font files installed on the host")
def test_different_font_files_give_different_cells_or_glyphs():
    first, second = HOST_FONTS[:2]
    assert cell_size(13, first) != cell_size(13, second) or not np.array_equal(
        glyph_atlas("M@g", 13, first), glyph_atlas("M@g", 13, second)
    )


@host_fonts
@pytest.mark.parametrize("cell", CELLS)
def test_cell_height_is_the_line_height_of_a_real_font_size_with_that_character_width(font, cell):
    heights = {sum(f.getmetrics()) for f in _real_font_sizes(font, cell)}
    assert heights, "no font size gives this character width"
    assert cell_size(cell, font)[1] in heights


@host_fonts
@pytest.mark.parametrize("cell", [6, 7, 8, 10, 13, 16, 20])  # at 5 px antialiasing leaves no row above half coverage
def test_capital_letters_stand_on_the_font_baseline(font, cell):
    ascents = {f.getmetrics()[0] for f in _real_font_sizes(font, cell)}
    letter = glyph_atlas("M ", cell, font)[1]
    baseline = np.flatnonzero(letter.max(axis=1) > 0.5).max() + 1  # first row below the letter's feet
    assert baseline in ascents


def _ink_centre_offset(glyph: np.ndarray, width: float) -> float:
    """Horizontal distance in px from the middle of the cell to the centre of mass of the ink."""
    return (glyph.sum(axis=0) * (np.arange(glyph.shape[1]) + 0.5)).sum() / glyph.sum() - width / 2


def _font_intrinsic_offset(font: ImageFont.FreeTypeFont, ch: str) -> float:
    """Where Pillow itself puts the ink of `ch` relative to the middle of its advance: the font's own side bearings."""
    origin = 50  # px from the left edge of the scratch canvas to where the glyph's advance box starts
    canvas = Image.new("L", (200, 200), 0)
    ImageDraw.Draw(canvas).text((origin, 100), ch, fill=255, font=font, anchor="ls")
    return _ink_centre_offset(np.asarray(canvas, dtype=np.float64), font.getlength(ch)) - origin


@host_fonts
@pytest.mark.parametrize("cell", [7, 8, 10, 13, 16, 20])
def test_letters_sit_in_the_middle_of_the_cell_as_the_font_designed_them(font, cell):
    chars = "HIO|"
    width = cell_size(cell, font)[0]
    observed = np.mean([_ink_centre_offset(glyph_atlas(f"{ch} ", cell, font)[1], width) for ch in chars])
    designed = [np.mean([_font_intrinsic_offset(f, ch) for ch in chars]) for f in _real_font_sizes(font, cell)]
    assert min(abs(observed - d) for d in designed) < 0.4  # px; a glyph drawn one pixel off shows about 1.0


@host_fonts
@pytest.mark.parametrize("cell", CELLS)
def test_a_capital_letter_leaves_room_above_and_below(font, cell):
    rows = np.flatnonzero(glyph_atlas("M ", cell, font)[1].max(axis=1) > 0)
    assert rows.min() >= 1
    assert rows.max() <= cell_size(cell, font)[1] - 2


@pytest.mark.parametrize("cell", CELLS)
def test_a_solid_glyph_has_full_coverage_so_the_ink_level_is_exact(cell):
    atlas = glyph_atlas(" .#█", cell)
    assert atlas.max() == 1.0
    assert atlas.min() == 0.0


@pytest.mark.parametrize("cell", range(5, 21))
def test_cell_height_is_between_one_and_a_fifth_and_two_and_a_half_widths(cell):
    width, height = cell_size(cell)
    assert 1.2 <= height / width <= 2.6
    assert cell_size(cell) == (width, height)  # deterministic
    assert glyph_atlas(" .", cell).shape[1] == height
