"""Tests of `ops/textart.py`: a picture redrawn as a grid of characters.

Expected pictures come from `ops.glyphs` (the glyph shapes) and from the colours given, never from `text_art` itself.
"""

import numpy as np
import pytest

from photo_fx.ops.glyphs import cell_size, glyph_atlas
from photo_fx.ops.textart import check_palette, text_art

CHARS = " .:-=+*#%@"
BLACK, WHITE = np.zeros(3, np.float32), np.ones(3, np.float32)


def draw(rgb, *, fg=WHITE, background=BLACK, photo=0.0, cell=8, chars=CHARS, gamma=1.0):
    return text_art(rgb, cell=cell, chars=chars, fg=fg, background=background, gamma=gamma, photo=photo, font=None)


def test_a_black_picture_on_black_paper_stays_blank():
    width, height = cell_size(8)
    out = draw(np.zeros((2 * height, 3 * width, 3), np.float32))
    assert out.max() == 0.0


def test_a_white_picture_on_black_paper_is_the_densest_glyph_on_every_cell():
    width, height = cell_size(8)
    out = draw(np.ones((height, 2 * width, 3), np.float32))
    densest = np.tile(glyph_atlas(CHARS, 8, None)[-1], (1, 2))
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], densest, atol=1e-6)


def test_on_light_paper_the_ramp_flips_so_a_white_picture_is_blank_paper():
    width, height = cell_size(8)
    out = draw(np.ones((height, width, 3), np.float32), fg=BLACK, background=WHITE)
    np.testing.assert_array_equal(out, np.ones_like(out))


def _glyph_drawn_for_grey(level: float, gamma: float) -> np.ndarray:
    """What `text_art` draws for one character cell of flat grey `level`, white ink on black paper (one channel)."""
    width, height = cell_size(8)
    return draw(np.full((height, width, 3), level, np.float32), gamma=gamma)[..., 0]


def test_a_cell_gets_the_glyph_nearest_its_ink_amount():
    atlas = glyph_atlas(CHARS, 8, None)  # 10 glyphs, index 0 = least ink
    # grey 0.3 on black paper wants 0.3 of full ink: 0.3 * 9 = 2.7 glyph steps, nearest is glyph 3 (not 2)
    np.testing.assert_allclose(_glyph_drawn_for_grey(0.3, gamma=1.0), atlas[3], atol=1e-6)
    assert not np.allclose(atlas[3], atlas[2])  # the two candidates really differ


def test_gamma_is_applied_to_the_ink_amount():
    atlas = glyph_atlas(CHARS, 8, None)
    # grey 0.6: gamma 1 gives 0.6 * 9 = 5.4 -> glyph 5; gamma 2 gives 0.6**2 = 0.36, 0.36 * 9 = 3.24 -> glyph 3
    np.testing.assert_allclose(_glyph_drawn_for_grey(0.6, gamma=1.0), atlas[5], atol=1e-6)
    np.testing.assert_allclose(_glyph_drawn_for_grey(0.6, gamma=2.0), atlas[3], atol=1e-6)
    assert not np.allclose(atlas[5], atlas[3])


def test_with_one_ink_colour_every_pixel_is_a_mix_of_ink_and_paper(photo):
    red, blue = np.array([1, 0, 0], np.float32), np.array([0, 0, 1], np.float32)
    out = draw(photo, fg=red, background=blue)
    assert out[..., 1].max() == 0.0  # neither colour has green
    np.testing.assert_allclose(out[..., 0] + out[..., 2], 1.0, atol=1e-6)  # ink share + paper share = 1
    # both colours really appear
    assert out[..., 0].max() > 0.5
    assert out[..., 2].max() == 1.0


def test_with_photo_one_the_picture_shows_wherever_there_is_no_ink():
    width, height = cell_size(8)
    picture = np.zeros((height, 2 * width, 3), np.float32)
    picture[:, :, 1] = 0.01  # nearly black: the lightest glyph (a space, no ink), but not equal to the paper
    out = draw(picture, photo=1.0)
    np.testing.assert_array_equal(out, picture)
    assert not np.array_equal(draw(picture, photo=0.0), picture)  # at 0 the paper replaces the picture


def test_the_photos_own_colours_are_used_when_no_ink_colour_is_given():
    width, height = cell_size(8)
    green = np.zeros((height, width, 3), np.float32)
    green[..., 1] = 1.0
    out = draw(green, fg=None)
    # no red or blue ink on black paper
    assert out[..., 0].max() == 0.0
    assert out[..., 2].max() == 0.0
    assert out[..., 1].max() > 0.5  # but plenty of green ink (antialiased glyphs never quite reach full coverage)


def test_sizes_off_the_character_grid_keep_their_shape_and_type():
    odd = np.random.default_rng(0).random((37, 53, 3), dtype=np.float32)
    out = draw(odd)
    assert out.shape == odd.shape
    assert out.dtype == np.float32


@pytest.mark.parametrize("chars", ["01", " @", "aab", " ░▒▓█"])
def test_a_palette_of_two_or_more_distinct_characters_is_accepted_unchanged(chars):
    assert check_palette(chars) == chars


@pytest.mark.parametrize("chars", ["", "a", "aaaa", "  "])
def test_a_palette_without_two_distinct_characters_is_refused_naming_it(chars):
    with pytest.raises(ValueError, match="two different characters"):
        check_palette(chars)
