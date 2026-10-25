"""Behaviour of the `starfield` effect: a generated sky of stars that replaces the picture and keeps only its size."""

from functools import partial
from typing import Any

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "starfield"
SIZE = 160
BLACK_SKY = {"sky": "#000000", "color": "#ffffff"}  # so that red is the light of the stars themselves


def stars(side: int = SIZE, **settings: Any) -> np.ndarray:
    """The red channel of a sky of white stars on pure black, (side, side)."""
    return registry.apply(NAME, np.zeros((side, side, 3), np.float32), **{**BLACK_SKY, **settings})[..., 0]


@pytest.fixture
def lone_star(lone_particle):
    """`lone_star(seed, **settings)`: the light of one star, not cut off by the picture's edge."""
    return partial(lone_particle, lambda settings: stars(**settings), count=1)


def test_with_no_stars_the_sky_is_one_flat_colour() -> None:
    out = registry.apply(NAME, np.ones((20, 30, 3), np.float32), count=0, sky="#336699")
    expected = np.array([0x33, 0x66, 0x99], np.float32) / 255
    np.testing.assert_allclose(out, np.broadcast_to(expected, out.shape), atol=1e-6)


@pytest.mark.parametrize("side", [200, 400])
def test_count_stars_are_made_whatever_the_size_of_the_picture(side: int) -> None:
    """The count is absolute: 40 small stars far apart are 40 separate dots."""
    patches = ndimage.label(stars(side, count=40, size=2.0, depth_range=0.0, seed=5) > 0, structure=np.ones((3, 3)))[1]
    assert patches == pytest.approx(40, abs=3)  # a few may touch


def test_a_star_is_a_dot_as_wide_and_as_bright_as_its_depth(lone_star) -> None:
    """With depth range d a star's scale is s = 1 - d x u for u random in 0..1, so from 1 - d to 1: it is `size` x s
    wide and s bright (so the nearest, s = 1, are the biggest and brightest)."""
    peaks = []
    for seed in range(0, 120, 6):
        star = lone_star(seed, size=12.0, depth_range=0.6)
        scale = star.max()
        peaks.append(scale)
        radius = np.sqrt((star > 0).sum() / np.pi) - 0.5  # a disc of radius r touches pixels within r + 0.5
        assert radius == pytest.approx(6.0 * scale, abs=0.5)
    assert 0.4 <= min(peaks) < 0.7
    assert 0.9 < max(peaks) <= 1.0


@pytest.mark.parametrize(("depth_range", "lowest"), [(0.0, 1.0), (0.5, 0.5), (1.0, 0.0)])
def test_depth_range_sets_how_far_the_stars_range_below_full_brightness(depth_range: float, lowest: float) -> None:
    """Every star's brightness lies from 1 - depth_range to 1, and the brightest of many is almost 1."""
    sky = stars(count=400, size=12.0, depth_range=depth_range, seed=2)
    patches, found = ndimage.label(sky > 0, structure=np.ones((3, 3)))
    peaks = np.array(ndimage.maximum(sky, patches, range(1, found + 1)))
    assert peaks.max() == pytest.approx(1.0, abs=0.01)
    assert peaks.min() >= lowest - 0.01
    assert peaks.min() <= lowest + 0.25  # and the dimmest of many is near the bottom of the range


def test_deeper_skies_are_dimmer_on_average() -> None:
    means = [stars(count=300, size=8.0, depth_range=depth_range, seed=3).sum() for depth_range in (0.0, 0.5, 1.0)]
    assert means[0] > means[1] > means[2]


def test_the_color_tints_the_stars_and_the_sky_shows_between_them() -> None:
    out = registry.apply(NAME, np.zeros((SIZE, SIZE, 3), np.float32), count=100, color="#ff0000", sky="#000080", seed=1)
    assert out[..., 0].max() > 0.9  # red stars
    np.testing.assert_allclose(out[..., 2].min(), 128 / 255, atol=1e-6)  # the sky's blue is the floor
    assert out[..., 1].max() == pytest.approx(out[..., 1].min())  # the stars add no green: the green is the sky's


def test_stars_are_screened_over_the_sky_so_they_never_make_it_darker() -> None:
    sky = np.array([0x20, 0x30, 0x60], np.float32) / 255
    out = registry.apply(NAME, np.zeros((SIZE, SIZE, 3), np.float32), count=200, sky="#203060", seed=4)
    assert (out >= sky - 1e-6).all()
    assert (out > sky + 0.1).any()
    # screen over mid grey 0.5 with the light l the same stars give on black: 1 - (1 - 0.5)(1 - l) = 0.5 + 0.5 l
    grey = registry.apply(
        NAME, np.zeros((SIZE, SIZE, 3), np.float32), count=200, sky="#808080", color="#ffffff", seed=4
    )
    np.testing.assert_allclose(grey[..., 0], (128 / 255) + (1 - 128 / 255) * stars(count=200, seed=4), atol=1e-6)
