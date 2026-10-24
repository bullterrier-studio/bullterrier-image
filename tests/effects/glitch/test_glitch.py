"""Behaviour of the `glitch` effect: sliding strips and a red/blue split."""

import numpy as np
import pytest

from photo_fx import RecipeError, fx
from photo_fx.core import registry

NAME = "glitch"
STILL = {"split": 0, "shift": 0.0}  # no sliding and no split


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
