"""Behaviour of the `embers` effect: glowing sparks that rise, each trailing a short streak."""

from functools import partial

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "embers"
SIZE = 160
WHITE = {"color": "#ffffff", "opacity": 1.0}


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the sparks add to a black picture, (h, w) in 0..1 (white sparks at full opacity unless `settings` say
    otherwise, so red is the light itself)."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **{**WHITE, **settings})[..., 0]


@pytest.fixture
def lone_ember(lone_particle):
    """`lone_ember(seed, **settings)`: the light of one spark, not cut off by the picture's edge."""
    return partial(lone_particle, light, count=1)


@pytest.mark.parametrize("size", [200, 400])
def test_count_embers_are_made_whatever_the_size_of_the_picture(size: int) -> None:
    """The count is absolute, not per area: 12 short thin sparks far apart are 12 separate patches of light."""
    mask = light({"count": 12, "size": 2.0, "length": 6.0, "seed": 5}, size=size) > 0
    assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(12, abs=1)


def test_more_embers_touch_more_of_the_picture() -> None:
    fractions = [(light({"count": count, "seed": 4}) > 0).mean() for count in (5, 20, 80, 320)]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time


@pytest.mark.parametrize("angle", [0.0, 15.0, 35.0, -25.0])
def test_a_spark_drifts_by_the_angle_from_straight_up_trailing_the_other_way(angle: float, lone_ember) -> None:
    """Rising, the spark moves tan(angle) px sideways per row up (to the right for a positive angle), so down the
    picture its streak moves -tan(angle) px per row."""
    streak = lone_ember(3, angle=angle, length=60.0, size=3.0)
    rows = np.flatnonzero(streak.sum(axis=1) > 0)
    column = (streak * np.arange(SIZE)).sum(axis=1)[rows] / streak.sum(axis=1)[rows]
    assert np.polyfit(rows, column, 1)[0] == pytest.approx(-np.tan(np.radians(angle)), abs=0.01)


def test_a_spark_is_longer_wider_and_brighter_by_the_same_depth(lone_ember) -> None:
    """A near spark has the full `size`, `length` and `opacity`, a far one half of each; so with s the share of any one
    of them, the streak spans (length + size) x s rows, is size x s wide at brightness opacity x s: peak = s, and the
    light across a row = size x s x s."""
    settings = {"angle": 0.0, "length": 50.0, "size": 6.0, "opacity": 0.8}
    depths = []
    for seed in range(6):
        streak = lone_ember(seed, **settings)
        by_brightness = streak.max() / 0.8
        depths.append(by_brightness)
        rows = np.flatnonzero(streak.sum(axis=1) > 0)
        by_length = (len(rows) - 1) / (50.0 + 6.0)  # the rounded ends reach half a pixel more at each end
        by_width = np.sqrt(streak.sum(axis=1)[rows[len(rows) // 2]] / (0.8 * 6.0))
        assert 0.5 <= by_brightness <= 1.0
        assert by_length == pytest.approx(by_brightness, abs=0.05)
        assert by_width == pytest.approx(by_brightness, abs=0.05)
    assert max(depths) - min(depths) > 0.2  # the depth differs from spark to spark


def test_without_length_a_spark_is_a_round_dot(lone_ember) -> None:
    dot = lone_ember(1, length=0.0, size=10.0)
    rows, columns = np.nonzero(dot)
    assert rows.max() - rows.min() == columns.max() - columns.min()


def test_a_softer_spark_has_a_wider_ramp_from_full_brightness_to_none(lone_ember) -> None:
    def ramp_pixels(softness: float) -> int:
        spark = lone_ember(2, size=30.0, length=0.0, softness=softness)
        return int(((spark > 0.05 * spark.max()) & (spark < 0.95 * spark.max())).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]
