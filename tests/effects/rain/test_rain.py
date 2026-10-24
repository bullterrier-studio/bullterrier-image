"""Behaviour of the `rain` effect: falling streaks screened over the picture."""

import itertools
from functools import partial

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "rain"
SIZE = 160
ONE_STREAK = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 streaks, rounded to one
WHITE = {"color": "#ffffff", "opacity": 1.0}


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the streaks add to a black picture: (h, w), 0..1 (white streaks, so red = the light itself)."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **{**WHITE, **settings})[..., 0]


@pytest.fixture
def lone_streak(lone_particle):
    """`lone_streak(seed, **settings)`: the light of one streak, not cut off by the picture's edge."""
    return partial(lone_particle, light, **ONE_STREAK)


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
def test_a_streak_leans_by_the_angle_from_vertical(angle: float, lone_streak) -> None:
    """The streak's pixel line moves sideways tan(angle) px per row down the picture (left for a negative angle)."""
    streak = lone_streak(seed=3, angle=angle, length=60.0, thickness=2.0)
    rows = np.flatnonzero(streak.sum(axis=1) > 0)
    column = (streak * np.arange(SIZE)).sum(axis=1)[rows] / streak.sum(axis=1)[rows]
    slope = np.polyfit(rows, column, 1)[0]
    assert slope == pytest.approx(np.tan(np.radians(angle)), abs=0.01)


def test_a_streak_is_longer_thicker_and_brighter_by_the_same_depth(lone_streak) -> None:
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


def test_no_streak_is_brighter_than_the_opacity_even_where_streaks_cross() -> None:
    """The nearest streaks are screened at `opacity`; where two cross the brighter shows, the light does not add up."""
    dense = light({"density": 5.0, "opacity": 0.5, "seed": 1})
    assert dense.max() == pytest.approx(0.5, abs=0.01)
