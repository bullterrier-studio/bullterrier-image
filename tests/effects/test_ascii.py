"""Tests of `effects/ascii.py`."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.ops.glyphs import cell_size, glyph_atlas


def test_ascii_atlas_is_sorted_from_least_to_most_ink():
    atlas = glyph_atlas("@.:", 10, None)  # given deliberately out of order
    ink = atlas.reshape(len(atlas), -1).mean(axis=1)
    assert list(ink) == sorted(ink) and ink[0] < ink[-1]


def test_ascii_bright_areas_get_dense_glyphs_on_dark_background():
    atlas = glyph_atlas(" .:-=+*#%@", 10, None)
    height, width = atlas.shape[1:]
    white, black = np.ones((height, width, 3), np.float32), np.zeros((height, width, 3), np.float32)
    on_dark = effects.apply("ascii", white, cell=10, fg="#ffffff", bg="#000000")
    assert on_dark.mean() == pytest.approx(atlas[-1].mean(), abs=1e-3)
    assert effects.apply("ascii", black, cell=10, fg="#ffffff", bg="#000000").max() == 0.0


def test_ascii_flips_the_ramp_on_a_light_background():
    chars = " .:-=+*#%@"
    atlas = glyph_atlas(chars, 10, None)
    black = np.zeros((*atlas.shape[1:], 3), np.float32)
    on_light = effects.apply("ascii", black, cell=10, chars=chars, fg="#000000", bg="#ffffff")
    # a dark picture on paper is the densest glyph in black ink, not blank paper
    assert on_light.mean() == pytest.approx(1 - atlas[-1].mean(), abs=1e-3)


def test_ascii_needs_two_distinct_characters(photo):
    with pytest.raises(ValueError, match="at least two"):
        effects.apply("ascii", photo, chars="aaaa")


def test_ascii_handles_sizes_that_are_not_a_multiple_of_the_cell():
    odd = np.random.default_rng(0).random((37, 53, 3), dtype=np.float32)
    assert effects.apply("ascii", odd, cell=8).shape == odd.shape


def test_ascii_photo_keeps_the_picture_visible_behind_the_text(photo):
    flat = effects.apply("ascii", photo, cell=8, photo=0.0)
    over = effects.apply("ascii", photo, cell=8, photo=1.0)
    assert np.abs(over - photo).mean() < np.abs(flat - photo).mean() / 2  # much closer to the original
    assert not np.allclose(over, photo)  # but the text is still drawn


def test_cell_size_matches_the_grid_ascii_draws_on():
    cell_w, cell_h = cell_size(8)
    assert glyph_atlas(" .:", 8, None).shape[1:] == (cell_h, cell_w)


def test_the_ascii_effects_are_in_their_categories():
    """ascii: where the effect catalogue lists them."""
    expected = {
        "ascii": "text",
    }
    assert {name: effects.category(name) for name in expected} == expected
