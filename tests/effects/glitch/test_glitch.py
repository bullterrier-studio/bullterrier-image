"""Behaviour of the `glitch` effect: sliding strips, a red/blue split, and strips turned to text."""

import numpy as np
import pytest

from photo_fx import effects, fx
from photo_fx.ops.glyphs import cell_size, glyph_atlas

NAME = "glitch"
STILL = {"split": 0, "shift": 0.0}  # no sliding and no split: only the text strips change anything


def _changed_rows(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    return (a != b).any(axis=(1, 2))


def test_glitch_only_moves_pixels_around(photo: np.ndarray) -> None:
    out = effects.apply(NAME, photo, seed=3)
    assert not np.array_equal(out, photo)
    for c in range(3):  # rolling permutes values within each channel
        np.testing.assert_array_equal(np.sort(out[..., c], axis=None), np.sort(photo[..., c], axis=None))


@pytest.mark.parametrize("split", [5, -7])
def test_split_pushes_red_and_blue_apart_by_that_many_pixels(photo: np.ndarray, split: int) -> None:
    out = effects.apply(NAME, photo, split=split, bands=0)
    np.testing.assert_array_equal(out[..., 0], np.roll(photo[..., 0], split, axis=1))
    np.testing.assert_array_equal(out[..., 1], photo[..., 1])
    np.testing.assert_array_equal(out[..., 2], np.roll(photo[..., 2], -split, axis=1))


def test_without_shift_and_split_nothing_moves(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(effects.apply(NAME, photo, bands=12, seed=2, **STILL), photo)


def test_a_band_is_its_height_share_of_the_picture_tall(photo: np.ndarray) -> None:
    # one band, exactly a quarter of the 96 rows, slid by up to half the width
    out = effects.apply(NAME, photo, split=0, bands=1, shift=0.5, band_min_height=0.25, band_max_height=0.25, seed=1)
    changed = np.flatnonzero(_changed_rows(out, photo))
    assert len(changed) == 24
    assert changed[-1] - changed[0] == 23  # one unbroken strip


def test_band_heights_spread_between_their_limits(photo: np.ndarray) -> None:
    # one band of a quarter to a half of the 96 rows, under 20 seeds: always 24..48 rows, and both short and tall ones
    heights = [
        _changed_rows(
            effects.apply(NAME, photo, split=0, bands=1, shift=0.5, band_min_height=0.25, band_max_height=0.5, seed=s),
            photo,
        ).sum()
        for s in range(20)
    ]
    assert min(heights) >= 24
    assert max(heights) <= 48
    assert min(heights) < 36 < max(heights)


def test_band_height_limits_given_the_wrong_way_round_mean_the_same_range(photo: np.ndarray) -> None:
    swapped = effects.apply(NAME, photo, band_min_height=0.2, band_max_height=0.05, seed=6)
    in_order = effects.apply(NAME, photo, band_min_height=0.05, band_max_height=0.2, seed=6)
    np.testing.assert_array_equal(swapped, in_order)
    assert not np.array_equal(swapped, effects.apply(NAME, photo, seed=6))  # and the limits are really used


def test_text_strips_change_only_some_rows_and_leave_the_sliding_alone(photo: np.ndarray) -> None:
    plain = effects.apply(NAME, photo, seed=3, ascii_share=0.0)
    texty = effects.apply(NAME, photo, seed=3, ascii_share=0.5, ascii_cell=6)
    changed = _changed_rows(plain, texty)
    assert changed.any(), "no text was drawn"
    assert not changed.all(), "text covered everything, so the sliding strips are not visible any more"


@pytest.mark.parametrize("share", [0.0, 0.5], ids=["sliding only", "with text"])
def test_it_is_deterministic_and_seed_dependent(photo: np.ndarray, share: float) -> None:
    a = effects.apply(NAME, photo, seed=5, ascii_share=share)
    np.testing.assert_array_equal(a, effects.apply(NAME, photo, seed=5, ascii_share=share))
    assert not np.array_equal(a, effects.apply(NAME, photo, seed=6, ascii_share=share))


def test_text_share_is_a_fraction_of_the_strips(photo: np.ndarray) -> None:
    plain = effects.apply(NAME, photo, seed=3)
    some = _changed_rows(effects.apply(NAME, photo, seed=3, ascii_share=0.3), plain)
    most = _changed_rows(effects.apply(NAME, photo, seed=3, ascii_share=1.0), plain)
    assert most.sum() >= some.sum() > 0


def test_any_share_above_zero_turns_at_least_one_strip_to_text(photo: np.ndarray) -> None:
    # 1 % of 7 strips is 0.07 of a strip: rounded up, one strip
    plain = effects.apply(NAME, photo, seed=3)
    assert _changed_rows(effects.apply(NAME, photo, seed=3, ascii_share=0.01), plain).any()


@pytest.mark.parametrize(("cell", "rows"), [(4, 3), (4, 5), (3, 2)])
def test_a_text_strip_is_widened_to_hold_its_rows_of_characters(photo: np.ndarray, cell: int, rows: int) -> None:
    # a one-row band turned to text grows to `rows` rows of characters, each counted as twice the character width
    text = effects.apply(
        NAME, photo, bands=1, band_min_height=0.0, band_max_height=0.0, ascii_share=1.0, ascii_cell=cell,
        ascii_rows=rows, **STILL,
    )  # fmt: skip
    assert _changed_rows(text, photo).sum() == rows * 2 * cell


def _all_text(picture: np.ndarray, cell: int, **params: object) -> np.ndarray:
    """The glitch of `picture` with a single text strip as tall as the picture (so all of it is text)."""
    rows = -(-picture.shape[0] // (2 * cell))
    return effects.apply(NAME, picture, bands=1, ascii_share=1.0, ascii_cell=cell, ascii_rows=rows, **STILL, **params)


def test_the_text_palette_ink_and_paper_are_the_ones_given() -> None:
    width, height = cell_size(8)
    white = np.ones((2 * height, 3 * width, 3), np.float32)
    out = _all_text(white, 8, ascii_chars=". ", ascii_ink="#ff0000", ascii_paper="#000000")
    dot = np.tile(glyph_atlas(" .", 8, None)[-1], (2, 3))  # white on black paper: the densest of the palette
    np.testing.assert_allclose(out[..., 0], dot, atol=1e-6)
    assert out[..., 1:].max() == 0.0  # red ink on black paper, no green or blue


def test_a_dark_strip_on_dark_paper_is_the_paper_colour() -> None:
    black = np.zeros((40, 50, 3), np.float32)
    out = _all_text(black, 5, ascii_paper="#336699")
    np.testing.assert_allclose(out, np.broadcast_to([0x33 / 255, 0x66 / 255, 0x99 / 255], out.shape), atol=1e-6)


def test_a_lower_text_gamma_puts_down_more_ink() -> None:
    grey = np.full((48, 64, 3), 0.5, np.float32)
    light, heavy = _all_text(grey, 4, ascii_gamma=3.0), _all_text(grey, 4, ascii_gamma=0.3)
    assert heavy.mean() > light.mean() + 0.05


def test_ascii_photo_keeps_strip_detail(photo: np.ndarray) -> None:
    flat = effects.apply(NAME, photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=0.0)
    over = effects.apply(NAME, photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=1.0)
    plain = effects.apply(NAME, photo, seed=3)
    assert np.abs(over - plain).mean() < np.abs(flat - plain).mean()


def test_a_text_font_that_cannot_be_opened_is_reported(photo: np.ndarray, tmp_path) -> None:
    with pytest.raises(OSError, match="cannot open"):  # Pillow's wording for a font file it cannot read
        effects.apply(NAME, photo, ascii_share=1.0, ascii_font=str(tmp_path / "missing.ttf"))


def test_a_text_palette_needs_two_distinct_characters_when_it_is_written() -> None:
    with pytest.raises(ValueError, match="ascii_chars"):
        fx.glitch(ascii_chars="..")
    assert fx.glitch(ascii_chars="01").params == {"ascii_chars": "01"}


def test_glitch_is_a_glitch_effect() -> None:
    assert effects.category(NAME) == "glitch"
