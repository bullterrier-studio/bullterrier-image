import numpy as np
import pytest
from PIL import Image, ImageDraw, ImageFont

from bullterrier_image import RecipeError, fx
from bullterrier_image.ops.glyphs import FONT, cell_size, glyph_atlas, paint

CELLS = [5, 8, 13, 20]


def ink(atlas: np.ndarray) -> np.ndarray:
    return atlas.sum(axis=(1, 2))


@pytest.mark.parametrize("cell", [1, 5, 40])
def test_cell_width_is_the_requested_width(cell):
    assert cell_size(cell)[0] == cell


def test_cell_height_grows_with_the_width():
    heights = [cell_size(c)[1] for c in [1, 5, 40]]
    assert heights == sorted(set(heights))  # strictly increasing


@pytest.mark.parametrize("cell", [5, 8, 13])
def test_atlas_shape_matches_cell_size(cell):
    width, height = cell_size(cell)
    atlas = glyph_atlas("a. #", cell)
    assert atlas.shape == (4, height, width)
    assert atlas.dtype == np.float32
    assert atlas.min() >= 0.0
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
    with pytest.raises(ValueError, match="read-only"):
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


# --- the bundled font and glyph geometry ---------------------------------------------------------------------------
# The font is a fixed asset, so these are checked against Pillow itself, reading the same file.


def _real_font_sizes(cell: int) -> list[ImageFont.FreeTypeFont]:
    """Every font size Pillow itself says has an 'M' `cell` px wide (+-0.5): the independent oracle for the geometry."""
    sizes = [ImageFont.truetype(FONT, size) for size in range(4, 200)]
    return [f for f in sizes if abs(f.getlength("M") - cell) <= 0.5]


@pytest.mark.parametrize("cell", CELLS)
def test_cell_height_is_the_line_height_of_a_real_font_size_with_that_character_width(cell):
    heights = {sum(f.getmetrics()) for f in _real_font_sizes(cell)}
    assert heights, "no font size gives this character width"
    assert cell_size(cell)[1] in heights


@pytest.mark.parametrize("cell", [6, 7, 8, 10, 13, 16, 20])  # at 5 px antialiasing leaves no row above half coverage
def test_capital_letters_stand_on_the_font_baseline(cell):
    ascents = {f.getmetrics()[0] for f in _real_font_sizes(cell)}
    letter = glyph_atlas("M ", cell)[1]
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


@pytest.mark.parametrize("cell", [7, 8, 10, 13, 16, 20])
def test_letters_sit_in_the_middle_of_the_cell_as_the_font_designed_them(cell):
    chars = "HIO|"
    width = cell_size(cell)[0]
    observed = np.mean([_ink_centre_offset(glyph_atlas(f"{ch} ", cell)[1], width) for ch in chars])
    designed = [np.mean([_font_intrinsic_offset(f, ch) for ch in chars]) for f in _real_font_sizes(cell)]
    assert min(abs(observed - d) for d in designed) < 0.4  # px; a glyph drawn one pixel off shows about 1.0


@pytest.mark.parametrize("cell", CELLS)
def test_a_capital_letter_leaves_room_above_and_below(cell):
    rows = np.flatnonzero(glyph_atlas("M ", cell)[1].max(axis=1) > 0)
    assert rows.min() >= 1
    assert rows.max() <= cell_size(cell)[1] - 2


@pytest.mark.parametrize("cell", range(5, 21))
def test_cell_height_is_between_one_and_a_fifth_and_two_and_three_quarter_widths(cell):
    width, height = cell_size(cell)
    assert 1.2 <= height / width <= 2.75
    assert glyph_atlas(" .", cell).shape[1] == height


# --- what the bundled font covers ----------------------------------------------------------------------------------

DEFAULT_RAMP = " .:-=+*#%@"
LONG_RAMP = " .`:,;-_~^!i+*Il1tfjrxnuvczXYUJCLQ0OZmwqpdbkhao#MW&8%B@$"  # the 60-character ramp of examples/recipes
BLOCKS = "█▓▒░▀▄"
MISSING = "\uffff"  # a Unicode noncharacter: no font has it, so a font draws its missing-glyph box for it


@pytest.mark.parametrize(
    "palette",
    [
        pytest.param(DEFAULT_RAMP, id="default ramp"),
        pytest.param(LONG_RAMP, id="long ramp"),
        pytest.param(BLOCKS, id="block elements"),
    ],
)
def test_every_character_of_a_palette_is_drawn_not_the_missing_glyph_box(palette):
    box = glyph_atlas(f" {MISSING}", 20)[1]
    assert box.sum() > 0  # the oracle: the box has ink
    for ch in palette:
        drawn = glyph_atlas(f"{ch}{MISSING}", 20)
        assert len({bytes(glyph) for glyph in drawn}) == len(drawn), f"{ch!r} looks like the missing-glyph box"


def test_the_ramps_are_non_blank_except_the_space():
    for ramp in (DEFAULT_RAMP, LONG_RAMP):
        totals = ink(glyph_atlas(ramp, 12))
        assert (totals == 0).sum() == 1  # the space, and only it
        assert (totals > 0).sum() == len(set(ramp)) - 1


@pytest.mark.parametrize("cell", CELLS)
def test_a_solid_glyph_has_full_coverage_so_the_ink_level_is_exact(cell):
    atlas = glyph_atlas(" .#█", cell)
    assert atlas.max() == 1.0
    assert atlas.min() == 0.0


@pytest.mark.parametrize("cell", [13, 20, 40])  # at smaller cells the font's own edge rows are a larger share
def test_the_full_block_fills_the_cell_and_the_shades_step_between_blank_and_full(cell):
    shades = [glyph_atlas(f" {ch}", cell)[1].mean() for ch in "░▒▓█"]
    assert shades[-1] > 0.95
    assert shades == sorted(shades)
    assert 0 < shades[0] < shades[-1]


def test_the_environment_cannot_change_what_is_drawn(monkeypatch):
    cell = 23  # a width no other test draws at, so nothing cached earlier can answer for it
    monkeypatch.setenv("BT_IMAGE_FONTS", "/no/such/dir/Font.ttf")
    heights = {sum(f.getmetrics()) for f in _real_font_sizes(cell)}
    assert cell_size(cell)[1] in heights
    letter = glyph_atlas("M ", cell)[1]
    assert np.flatnonzero(letter.max(axis=1) > 0.5).max() + 1 in {f.getmetrics()[0] for f in _real_font_sizes(cell)}


@pytest.mark.parametrize("effect", ["ascii", "disperse"])
def test_a_glyph_effect_takes_no_font_so_an_old_recipe_naming_one_is_refused(effect):
    with pytest.raises(RecipeError, match="font"):
        getattr(fx, effect)(font="/Library/Fonts/Menlo.ttc")
