"""Behaviour of the `rain` effect: falling streaks screened over the picture."""

import itertools

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "rain"
SIZE = 160
ONE_STREAK = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 streaks, rounded to one
WHITE = {"color": "#ffffff", "opacity": 1.0}


def black(size: int = SIZE) -> np.ndarray:
    return np.zeros((size, size, 3), np.float32)


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the streaks add to a black picture: (h, w), 0..1 (white streaks, so red = the light itself)."""
    return registry.apply(NAME, black(size), **{**WHITE, **settings})[..., 0]


def lone_streak(seed: int, **settings: float) -> np.ndarray:
    """The light of the single streak of the first seed from `seed` on that is not cut off by the picture's edge."""
    for candidate in range(seed, seed + 50):
        streak = light({**ONE_STREAK, "seed": candidate, **settings})
        if streak[0].max() == 0 == streak[-1].max() == streak[:, 0].max() == streak[:, -1].max():
            return streak
    raise AssertionError("every one of 50 seeds put the streak on the picture's edge")


@pytest.mark.parametrize("color", ["#ffffff", "#e6eefa"])
def test_no_density_leaves_the_picture_unchanged(color: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, density=0.0, color=color)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


def test_more_density_touches_more_of_the_picture_and_keeps_the_streaks_it_had() -> None:
    masks = [light({"density": density, "seed": 4}) > 0 for density in (0.1, 0.3, 1.0, 3.0)]
    fractions = [mask.mean() for mask in masks]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time
    for fewer, more in itertools.pairwise(masks):
        assert not (fewer & ~more).any()  # a streak at a lower density is still there at a higher one


def test_the_number_of_streaks_is_the_density_per_1000_square_pixels() -> None:
    """Short thin streaks far apart: density 0.1 on 400 x 400 px = 160 000 px^2 is 16 streaks, density 0.3 is 48."""
    for density, streaks in ((0.1, 16), (0.3, 48)):
        mask = light({"density": density, "length": 8.0, "thickness": 1.0, "seed": 6}, size=400) > 0
        assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(streaks, rel=0.15)


@pytest.mark.parametrize("angle", [0.0, 12.0, 30.0, -20.0])
def test_a_streak_leans_by_the_angle_from_vertical(angle: float) -> None:
    """The streak's pixel line moves sideways tan(angle) px per row down the picture (left for a negative angle)."""
    streak = lone_streak(seed=3, angle=angle, length=60.0, thickness=2.0)
    rows = np.flatnonzero(streak.sum(axis=1) > 0)
    column = (streak * np.arange(SIZE)).sum(axis=1)[rows] / streak.sum(axis=1)[rows]
    slope = np.polyfit(rows, column, 1)[0]
    assert slope == pytest.approx(np.tan(np.radians(angle)), abs=0.01)


def test_a_streak_is_longer_thicker_and_brighter_by_the_same_depth() -> None:
    """A near streak is drawn at the full `length`, `thickness` and `opacity`, a far one at half of each (depth 0). So
    for any streak the fractions s of its length, of its thickness and of its peak opacity are equal: its extent down
    the picture is length x s, the light across one row is opacity x thickness x s x s (a width of thickness x s at a
    brightness of opacity x s)."""
    settings = {"angle": 0.0, "length": 80.0, "thickness": 6.0, "opacity": 0.8}
    for seed in range(6):
        streak = lone_streak(seed, **settings)
        brightness = streak.max()
        rows = np.flatnonzero(streak.sum(axis=1) > 0)
        by_length = (len(rows) - 6.0) / 80.0  # less the rounded ends, about half the thickness each
        by_brightness = brightness / 0.8
        by_width = np.sqrt(streak.sum(axis=1)[rows[len(rows) // 2]] / (0.8 * 6.0))
        assert 0.5 <= by_brightness <= 1.0
        assert by_length == pytest.approx(by_brightness, abs=0.05)
        assert by_width == pytest.approx(by_brightness, abs=0.05)


@pytest.mark.parametrize("length", [10.0, 20.0, 40.0])
def test_longer_streaks_cover_more_of_the_picture(length: float) -> None:
    shorter = light({"density": 0.5, "length": length, "seed": 1}).sum()
    longer = light({"density": 0.5, "length": length * 2, "seed": 1}).sum()
    assert longer > 1.5 * shorter


def test_rain_only_adds_light_and_never_darkens(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, density=2.0, seed=2)
    assert (out >= photo).all()
    assert (out > photo).any()
    assert registry.apply(NAME, black(), density=1.0, seed=2).max() > 0.1  # and shows on a black picture


def test_the_streaks_are_screened_over_the_picture_so_a_bright_picture_stays_within_range() -> None:
    """Screen over mid grey 0.5 with light l gives 1 - (1 - 0.5)(1 - l) = 0.5 + 0.5 l, and over white stays white."""
    on_black = light({"density": 2.0, "seed": 3})
    grey = registry.apply(NAME, np.full((SIZE, SIZE, 3), 0.5, np.float32), density=2.0, seed=3, **WHITE)
    np.testing.assert_allclose(grey[..., 0], 0.5 + 0.5 * on_black, atol=1e-6)
    white = registry.apply(NAME, np.ones((SIZE, SIZE, 3), np.float32), density=2.0, seed=3, **WHITE)
    np.testing.assert_array_equal(white, 1.0)


def test_the_color_tints_the_streaks_and_the_opacity_scales_their_light() -> None:
    red = registry.apply(NAME, black(), density=1.0, color="#ff0000", opacity=1.0, seed=1)
    assert red[..., 0].max() > 0.4
    assert red[..., 1:].max() == 0
    half = registry.apply(NAME, black(), density=1.0, color="#ff0000", opacity=0.5, seed=1)
    np.testing.assert_allclose(half[..., 0], red[..., 0] / 2, atol=1e-6)


def test_no_streak_is_brighter_than_the_opacity_even_where_streaks_cross() -> None:
    """The nearest streaks are screened at `opacity`; where two cross the brighter shows, the light does not add up."""
    dense = light({"density": 5.0, "opacity": 0.5, "seed": 1})
    assert dense.max() == pytest.approx(0.5, abs=0.01)


@pytest.mark.parametrize("shape", [(60, 240), (240, 60)], ids=["wide", "tall"])
def test_the_streaks_are_spread_over_the_whole_of_a_wide_or_a_tall_picture(shape: tuple[int, int]) -> None:
    lit = registry.apply(NAME, np.zeros((*shape, 3), np.float32), density=3.0, seed=1)[..., 0] > 0
    for axis in (0, 1):  # the picture's rows, then its columns: every third of them holds some streaks
        thirds = np.array_split(lit.any(axis=1 - axis), 3)
        assert all(third.any() for third in thirds)


def test_the_same_seed_gives_the_same_rain_and_another_seed_other_rain(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert (registry.apply(NAME, photo, seed=1) != registry.apply(NAME, photo, seed=2)).any()
