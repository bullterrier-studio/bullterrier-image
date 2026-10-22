"""Behaviour of the `ascii` effect.

Expected pictures come from `ops.glyphs` (the glyph shapes) and the colours given, never from the effect itself.
"""

import numpy as np
import pytest

from photo_fx import effects, fx
from photo_fx.ops.glyphs import cell_size, glyph_atlas

NAME = "ascii"
CHARS = " .:-=+*#%@"  # the default palette, spelt out so a changed default is noticed


def _flat(level: float, cell: int, rows: int = 1, cols: int = 1) -> np.ndarray:
    """A flat grey picture exactly `rows` x `cols` characters of width `cell`."""
    width, height = cell_size(cell)
    return np.full((rows * height, cols * width, 3), level, np.float32)


def test_bright_areas_get_the_densest_glyph_on_dark_paper() -> None:
    densest = glyph_atlas(CHARS, 7, None)[-1]
    out = effects.apply(NAME, _flat(1.0, 7), cell=7, ink="#ffffff", paper="#000000")
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], densest, atol=1e-6)


def test_dark_areas_on_dark_paper_are_blank_paper() -> None:
    out = effects.apply(NAME, _flat(0.0, 7, rows=2, cols=3), cell=7, paper="#336699")
    np.testing.assert_allclose(out, np.broadcast_to([0x33 / 255, 0x66 / 255, 0x99 / 255], out.shape), atol=1e-6)


def test_the_ramp_flips_on_light_paper() -> None:
    densest = glyph_atlas(CHARS, 10, None)[-1]
    out = effects.apply(NAME, _flat(0.0, 10), cell=10, ink="#000000", paper="#ffffff")
    # a dark picture on paper is the densest glyph in black ink, not blank paper
    np.testing.assert_allclose(out[..., 0], 1 - densest, atol=1e-6)


def test_one_ink_colour_paints_every_character() -> None:
    out = effects.apply(NAME, _flat(1.0, 8, cols=2), cell=8, ink="#ff0000", paper="#000000")
    assert out[..., 1:].max() == 0.0  # no green or blue anywhere
    assert out[..., 0].max() > 0.5


def test_gamma_bends_the_ink_amount() -> None:
    atlas = glyph_atlas(CHARS, 8, None)  # 10 glyphs, index 0 = least ink
    # grey 0.6: gamma 1 wants 0.6 * 9 = 5.4 -> glyph 5; gamma 2 wants 0.36 * 9 = 3.24 -> glyph 3
    grey = _flat(0.6, 8)
    np.testing.assert_allclose(effects.apply(NAME, grey, cell=8, ink="#ffffff")[..., 0], atlas[5], atol=1e-6)
    np.testing.assert_allclose(effects.apply(NAME, grey, cell=8, ink="#ffffff", gamma=2.0)[..., 0], atlas[3], atol=1e-6)


def test_the_palette_is_the_characters_given() -> None:
    dot = glyph_atlas(" .", 8, None)[-1]
    out = effects.apply(NAME, _flat(1.0, 8), cell=8, chars=". ", ink="#ffffff")  # any order
    np.testing.assert_allclose(out[..., 0], dot, atol=1e-6)


def test_a_space_in_the_palette_is_a_character_not_padding() -> None:
    # stripped, " @" would be the one character "@" and refused; kept, a dark picture is all spaces: no ink
    blank = effects.apply(NAME, _flat(0.0, 8), cell=8, chars=" @", ink="#ffffff")
    assert blank.max() == 0.0


def test_a_palette_needs_two_distinct_characters_when_it_is_written() -> None:
    with pytest.raises(ValueError, match="chars"):
        fx.ascii(chars="aaaa")
    assert fx.ascii(chars="ab").params == {"chars": "ab"}


def test_a_font_that_cannot_be_opened_is_reported(photo: np.ndarray, tmp_path) -> None:
    with pytest.raises(OSError, match="cannot open"):  # Pillow's wording for a font file it cannot read
        effects.apply(NAME, photo, font=str(tmp_path / "missing.ttf"))


def test_sizes_off_the_character_grid_keep_their_shape() -> None:
    odd = np.random.default_rng(0).random((37, 53, 3), dtype=np.float32)
    assert effects.apply(NAME, odd, cell=8).shape == odd.shape


def test_photo_keeps_the_picture_visible_behind_the_text(photo: np.ndarray) -> None:
    flat = effects.apply(NAME, photo, cell=8, photo=0.0)
    over = effects.apply(NAME, photo, cell=8, photo=1.0)
    assert np.abs(over - photo).mean() < np.abs(flat - photo).mean() / 2  # much closer to the original
    assert not np.allclose(over, photo)  # but the text is still drawn


def test_ascii_is_a_text_effect() -> None:
    assert effects.category(NAME) == "text"
