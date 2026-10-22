"""Behaviour of the `ascii` effect.

Expected pictures come from `ops.glyphs` (the glyph shapes) and the colours given, never from the effect itself. Every
test passes the palette it relies on.
"""

import numpy as np
import pytest

from photo_fx import fx
from photo_fx.core import registry
from photo_fx.ops.glyphs import cell_size, glyph_atlas

NAME = "ascii"
CHARS = " .:-=+*#%@"  # ten glyphs, index 0 (the space) = no ink


def _flat(level: float | tuple[float, float, float], cell: int, rows: float = 1, cols: float = 1) -> np.ndarray:
    """A flat picture `rows` x `cols` characters of width `cell` (fractions give a partial last cell)."""
    width, height = cell_size(cell)
    return np.full((round(rows * height), round(cols * width), 3), level, np.float32)


def test_bright_areas_get_the_densest_glyph_on_dark_paper() -> None:
    densest = glyph_atlas(CHARS, 7, None)[-1]
    out = registry.apply(NAME, _flat(1.0, 7), cell=7, chars=CHARS, ink="#ffffff", paper="#000000")
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], densest, atol=1e-6)


def test_dark_areas_on_dark_paper_are_blank_paper() -> None:
    out = registry.apply(NAME, _flat(0.0, 7, rows=2, cols=3), cell=7, chars=CHARS, paper="#336699")
    np.testing.assert_allclose(out, np.broadcast_to([0x33 / 255, 0x66 / 255, 0x99 / 255], out.shape), atol=1e-6)


def test_the_ramp_flips_on_light_paper() -> None:
    densest = glyph_atlas(CHARS, 10, None)[-1]
    out = registry.apply(NAME, _flat(0.0, 10), cell=10, chars=CHARS, ink="#000000", paper="#ffffff")
    # a dark picture on paper is the densest glyph in black ink, not blank paper
    np.testing.assert_allclose(out[..., 0], 1 - densest, atol=1e-6)


def test_one_ink_colour_paints_every_character() -> None:
    out = registry.apply(NAME, _flat(1.0, 8, cols=2), cell=8, chars=CHARS, ink="#ff0000", paper="#000000")
    assert out[..., 1:].max() == 0.0  # no green or blue anywhere
    assert out[..., 0].max() > 0.5


@pytest.mark.parametrize(
    ("colour", "glyph", "ink"),
    [
        # strongest channel 0.9: scaled to full, (0.9, 0.45, 0.3) / 0.9; luminance 0.535 * 9 = 4.8 -> glyph 5
        ((0.9, 0.45, 0.3), 5, (1.0, 0.5, 1 / 3)),
        # strongest channel 0.3, under the 0.35 floor: scaled by 1 / 0.35; luminance 0.175 * 9 = 1.58 -> glyph 2
        ((0.3, 0.15, 0.06), 2, (0.3 / 0.35, 0.15 / 0.35, 0.06 / 0.35)),
    ],
    ids=["bright colour", "dark colour"],
)
def test_without_an_ink_each_character_takes_the_brightened_colour_under_it(colour, glyph, ink) -> None:
    coverage = glyph_atlas(CHARS, 8, None)[glyph]
    out = registry.apply(NAME, _flat(colour, 8), cell=8, chars=CHARS, paper="#000000")
    np.testing.assert_allclose(out, coverage[..., None] * np.array(ink, np.float32), atol=1e-5)  # float32 sums


def test_cells_cut_off_by_the_edge_are_measured_like_whole_ones() -> None:
    # a flat white picture one and a half characters each way: the cut-off cells are as white as the whole one
    densest = glyph_atlas(CHARS, 8, None)[-1]
    picture = _flat(1.0, 8, rows=1.5, cols=1.5)
    out = registry.apply(NAME, picture, cell=8, chars=CHARS, ink="#ffffff")
    expected = np.tile(densest, (2, 2))[: picture.shape[0], : picture.shape[1]]
    assert out.shape == picture.shape
    np.testing.assert_allclose(out[..., 0], expected, atol=1e-6)


def test_gamma_bends_the_ink_amount() -> None:
    atlas = glyph_atlas(CHARS, 8, None)
    # grey 0.6: gamma 1 wants 0.6 * 9 = 5.4 -> glyph 5; gamma 2 wants 0.36 * 9 = 3.24 -> glyph 3
    grey = _flat(0.6, 8)
    white_ink = {"cell": 8, "chars": CHARS, "ink": "#ffffff"}
    np.testing.assert_allclose(registry.apply(NAME, grey, **white_ink)[..., 0], atlas[5], atol=1e-6)
    np.testing.assert_allclose(registry.apply(NAME, grey, gamma=2.0, **white_ink)[..., 0], atlas[3], atol=1e-6)


def test_the_palette_is_the_characters_given() -> None:
    dot = glyph_atlas(" .", 8, None)[-1]
    out = registry.apply(NAME, _flat(1.0, 8), cell=8, chars=". ", ink="#ffffff")  # any order
    np.testing.assert_allclose(out[..., 0], dot, atol=1e-6)


def test_a_space_in_the_palette_is_a_character_not_padding() -> None:
    # trimmed, " @" would be the one character "@" and refused; kept, a dark picture is all spaces: no ink
    blank = registry.apply(NAME, _flat(0.0, 8), cell=8, chars=" @", ink="#ffffff")
    assert blank.max() == 0.0


def test_a_palette_needs_two_distinct_characters_when_it_is_written() -> None:
    with pytest.raises(ValueError, match="chars"):
        fx.ascii(chars="aaaa")
    assert fx.ascii(chars="ab").params == {"chars": "ab"}


def test_a_font_that_cannot_be_opened_is_reported(photo: np.ndarray, tmp_path) -> None:
    with pytest.raises(OSError):  # noqa: PT011  (the message is Pillow's own wording, not ours to pin)
        registry.apply(NAME, photo, font=str(tmp_path / "missing.ttf"))


def test_photo_keeps_the_picture_visible_behind_the_text(photo: np.ndarray) -> None:
    flat = registry.apply(NAME, photo, cell=8, chars=CHARS, photo=0.0)
    over = registry.apply(NAME, photo, cell=8, chars=CHARS, photo=1.0)
    assert np.abs(over - photo).mean() < np.abs(flat - photo).mean() / 2  # much closer to the original
    assert not np.allclose(over, photo)  # but the text is still drawn
