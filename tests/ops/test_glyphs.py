import numpy as np
import pytest

from photo_fx.effects import ascii as ascii_module
from photo_fx.ops import glyphs
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
    assert 0.0 <= atlas.min() and atlas.max() <= 1.0


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
    assert totals[-1] == totals.max() and totals[-1] > totals[:-1].max()
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


@pytest.mark.parametrize("rows, cols", [(1, 5), (4, 1), (3, 2)])
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


def test_ascii_module_keeps_the_names_other_code_imports():
    assert ascii_module.cell_size is glyphs.cell_size
    assert ascii_module._glyph_atlas is glyphs.glyph_atlas
