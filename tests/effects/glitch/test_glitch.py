"""Behaviour of the `glitch` effect: sliding strips, a red/blue split, and strips turned to text."""

import numpy as np
import pytest

from photo_fx import RecipeError, fx
from photo_fx.core import registry
from photo_fx.ops.glyphs import cell_size, glyph_atlas

NAME = "glitch"
STILL = {"split": 0, "shift": 0.0}  # no sliding and no split: only the text strips change anything
CHARS = " .:-=+*#%@"  # ten glyphs, index 0 (the space) = no ink


def _changed_rows(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    return (a != b).any(axis=(1, 2))


def test_glitch_only_moves_pixels_around(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, seed=3)
    assert not np.array_equal(out, photo)
    for c in range(3):  # rolling permutes values within each channel
        np.testing.assert_array_equal(np.sort(out[..., c], axis=None), np.sort(photo[..., c], axis=None))


@pytest.mark.parametrize("split", [5, -7])
def test_split_pushes_red_and_blue_apart_by_that_many_pixels(photo: np.ndarray, split: int) -> None:
    out = registry.apply(NAME, photo, split=split, bands=0)
    np.testing.assert_array_equal(out[..., 0], np.roll(photo[..., 0], split, axis=1))
    np.testing.assert_array_equal(out[..., 1], photo[..., 1])
    np.testing.assert_array_equal(out[..., 2], np.roll(photo[..., 2], -split, axis=1))


def test_without_shift_and_split_nothing_moves(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, bands=12, seed=2, **STILL), photo)


def test_a_band_is_its_height_share_of_the_picture_tall(photo: np.ndarray) -> None:
    # one band, exactly a quarter of the 96 rows, slid by up to half the width
    out = registry.apply(NAME, photo, split=0, bands=1, shift=0.5, band_min_height=0.25, band_max_height=0.25, seed=1)
    changed = np.flatnonzero(_changed_rows(out, photo))
    assert len(changed) == 24
    assert changed[-1] - changed[0] == 23  # one unbroken strip


def test_band_heights_spread_between_their_limits(photo: np.ndarray) -> None:
    # one band of a quarter to a half of the 96 rows, under 20 seeds: always 24..48 rows, and both short and tall ones
    heights = [
        _changed_rows(
            registry.apply(NAME, photo, split=0, bands=1, shift=0.5, band_min_height=0.25, band_max_height=0.5, seed=s),
            photo,
        ).sum()
        for s in range(20)
    ]
    assert min(heights) >= 24
    assert max(heights) <= 48
    assert min(heights) < 36 < max(heights)


def _slide(out: np.ndarray, photo: np.ndarray) -> int:
    """How far the one changed strip of `out` is `photo`'s strip rolled sideways (brute force over every distance);
    0 if nothing changed."""
    rows = _changed_rows(out, photo)
    if not rows.any():
        return 0
    width = photo.shape[1]
    matches = [d for d in range(-width + 1, width) if np.array_equal(out[rows], np.roll(photo[rows], d, axis=1))]
    assert matches, "the changed rows are not the picture's rows slid sideways"
    return min(matches, key=abs)  # a roll by d equals a roll by d - width: the shorter way round


def test_a_strip_slides_sideways_up_to_shift_of_the_width_either_way(photo: np.ndarray) -> None:
    # one band of a tenth of the height; the limit is 0.1 of the 128 px width = 12.8 px
    limit = 0.1 * photo.shape[1]
    slides = [
        _slide(
            registry.apply(NAME, photo, split=0, bands=1, shift=0.1, band_min_height=0.1, band_max_height=0.1, seed=s),
            photo,
        )
        for s in range(20)
    ]
    assert all(abs(d) <= limit for d in slides)
    assert min(slides) < 0 < max(slides)  # both directions
    assert max(abs(d) for d in slides) > 0.75 * limit  # and the slides reach most of the way to the limit


def test_band_height_limits_given_the_wrong_way_round_are_refused_naming_both() -> None:
    with pytest.raises(RecipeError, match="band_min_height") as raised:
        fx.glitch(band_min_height=0.2, band_max_height=0.05)
    assert "band_max_height" in str(raised.value)
    assert fx.glitch(band_min_height=0.2, band_max_height=0.2).params["band_min_height"] == 0.2  # equal is a range


def test_text_strips_cover_only_some_rows(photo: np.ndarray) -> None:
    plain = registry.apply(NAME, photo, seed=3, ascii_share=0.0)
    texty = registry.apply(NAME, photo, seed=3, ascii_share=0.5, ascii_cell=6)
    changed = _changed_rows(plain, texty)
    assert changed.any(), "no text was drawn"
    assert not changed.all(), "text covered everything, so the sliding strips are not visible any more"


@pytest.mark.parametrize("share", [0.0, 0.5], ids=["sliding only", "with text"])
def test_it_is_deterministic_and_seed_dependent(photo: np.ndarray, share: float) -> None:
    a = registry.apply(NAME, photo, seed=5, ascii_share=share)
    np.testing.assert_array_equal(a, registry.apply(NAME, photo, seed=5, ascii_share=share))
    assert not np.array_equal(a, registry.apply(NAME, photo, seed=6, ascii_share=share))


def test_text_share_is_a_fraction_of_the_strips(photo: np.ndarray) -> None:
    plain = registry.apply(NAME, photo, seed=3)
    some = _changed_rows(registry.apply(NAME, photo, seed=3, ascii_share=0.3), plain)
    most = _changed_rows(registry.apply(NAME, photo, seed=3, ascii_share=1.0), plain)
    assert most.sum() >= some.sum() > 0


def test_any_share_above_zero_turns_at_least_one_strip_to_text(photo: np.ndarray) -> None:
    # 1 % of 7 strips is 0.07 of a strip: rounded up, one strip
    plain = registry.apply(NAME, photo, seed=3)
    assert _changed_rows(registry.apply(NAME, photo, seed=3, ascii_share=0.01), plain).any()


@pytest.mark.parametrize(("cell", "rows"), [(4, 3), (4, 5), (3, 2)])
def test_a_text_strip_is_widened_to_hold_its_rows_of_characters(photo: np.ndarray, cell: int, rows: int) -> None:
    # a one-row band turned to text grows to `rows` rows of characters, each as tall as the font's character cell
    text = registry.apply(
        NAME, photo, bands=1, band_min_height=0.0, band_max_height=0.0, ascii_share=1.0, ascii_cell=cell,
        ascii_rows=rows, **STILL,
    )  # fmt: skip
    assert _changed_rows(text, photo).sum() == rows * cell_size(cell)[1]


def _all_text(picture: np.ndarray, cell: int, **params: object) -> np.ndarray:
    """The glitch of `picture` with a single text strip as tall as the picture (so all of it is text)."""
    rows = -(-picture.shape[0] // (2 * cell))
    return registry.apply(NAME, picture, bands=1, ascii_share=1.0, ascii_cell=cell, ascii_rows=rows, **STILL, **params)


def test_the_text_palette_ink_and_paper_are_the_ones_given() -> None:
    width, height = cell_size(8)
    white = np.ones((2 * height, 3 * width, 3), np.float32)
    out = _all_text(white, 8, ascii_chars=". ", ascii_ink="#ff0000", ascii_paper="#000000")
    dot = np.tile(glyph_atlas(" .", 8, None)[-1], (2, 3))  # white on black paper: the densest of the palette
    np.testing.assert_allclose(out[..., 0], dot, atol=1e-6)
    assert out[..., 1:].max() == 0.0  # red ink on black paper, no green or blue


def test_a_dark_strip_on_dark_paper_is_the_paper_colour() -> None:
    black = np.zeros((40, 50, 3), np.float32)
    out = _all_text(black, 5, ascii_chars=CHARS, ascii_paper="#336699")
    np.testing.assert_allclose(out, np.broadcast_to([0x33 / 255, 0x66 / 255, 0x99 / 255], out.shape), atol=1e-6)


def test_ascii_ink_floor_caps_how_much_a_dark_text_colour_is_brightened() -> None:
    width, height = cell_size(8)
    dark = np.full((height, 2 * width, 3), (0.3, 0.15, 0.06), np.float32)
    capped = _all_text(dark, 8, ascii_chars=CHARS, ascii_ink_floor=0.6)  # strongest channel 0.3: scaled by 1 / 0.6
    np.testing.assert_allclose(capped, _all_text(dark, 8, ascii_chars=CHARS, ascii_ink=[0.5, 0.25, 0.1]), atol=1e-5)
    assert capped[..., 0].max() > 0.25  # characters were drawn, so their colour was compared


def test_ascii_paper_threshold_decides_whether_mid_grey_paper_is_dark_or_light() -> None:
    grey = 128 / 255  # #808080: Rec. 709 luminance 0.502, just above the default threshold of 0.5
    width, height = cell_size(8)
    white = np.ones((height, 2 * width, 3), np.float32)
    settings = {"ascii_chars": CHARS, "ascii_ink": "#ff0000", "ascii_paper": "#808080"}
    light = _all_text(white, 8, **settings)  # light paper: white needs no ink
    np.testing.assert_allclose(light, np.full_like(light, grey), atol=1e-6)
    dark = _all_text(white, 8, ascii_paper_threshold=0.6, **settings)  # dark paper: white gets the densest glyph
    densest = np.tile(glyph_atlas(CHARS, 8, None)[-1], (1, 2))[..., None]
    np.testing.assert_allclose(dark, grey * (1 - densest) + np.array([1, 0, 0]) * densest, atol=1e-6)


def test_a_lower_text_gamma_puts_down_more_ink() -> None:
    grey = np.full((48, 64, 3), 0.5, np.float32)
    light = _all_text(grey, 4, ascii_chars=CHARS, ascii_gamma=3.0)
    heavy = _all_text(grey, 4, ascii_chars=CHARS, ascii_gamma=0.3)
    assert heavy.mean() > light.mean() + 0.05


def test_ascii_photo_keeps_strip_detail(photo: np.ndarray) -> None:
    flat = registry.apply(NAME, photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=0.0)
    over = registry.apply(NAME, photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=1.0)
    plain = registry.apply(NAME, photo, seed=3)
    assert np.abs(over - plain).mean() < np.abs(flat - plain).mean()


def test_a_text_font_that_cannot_be_opened_is_reported(photo: np.ndarray, tmp_path) -> None:
    with pytest.raises(OSError):  # noqa: PT011  (the message is Pillow's own wording, not ours to pin)
        registry.apply(NAME, photo, ascii_share=1.0, ascii_font=str(tmp_path / "missing.ttf"))


def test_a_text_palette_needs_two_distinct_characters_when_it_is_written() -> None:
    with pytest.raises(ValueError, match="ascii_chars"):
        fx.glitch(ascii_chars="..")
    assert fx.glitch(ascii_chars="01").params == {"ascii_chars": "01"}
