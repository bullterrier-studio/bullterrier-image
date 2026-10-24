"""Behaviour of the `floating_dust` effect: faint, soft, out-of-focus specks of light."""

from functools import partial

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "floating_dust"
SIZE = 160
WHITE = {"color": "#ffffff", "opacity": 1.0}


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the specks add to a black picture, (h, w) in 0..1 (white specks at full opacity, so red is the light
    itself) unless `settings` say otherwise."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **{**WHITE, **settings})[..., 0]


@pytest.fixture
def lone_speck(lone_particle):
    """`lone_speck(seed, **settings)`: the light of one speck, not cut off by the picture's edge."""
    return partial(lone_particle, light, count=1)


@pytest.mark.parametrize("size", [200, 400])
def test_count_specks_are_made_whatever_the_size_of_the_picture(size: int) -> None:
    """The count is absolute, not per area: 12 small specks far apart are 12 separate patches of light."""
    mask = light({"count": 12, "size": 6.0, "softness": 0.0, "seed": 5}, size=size) > 0
    assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(12, abs=1)


def test_more_specks_touch_more_of_the_picture() -> None:
    fractions = [(light({"count": count, "seed": 4}) > 0).mean() for count in (5, 20, 80, 320)]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time


def test_a_speck_is_a_soft_disc_as_big_and_bright_as_its_depth(lone_speck) -> None:
    """The disc is `size` x s wide and `opacity` x s bright, s from 0.5 to 1 (the speck's depth): full brightness
    inside, half of it at the radius `size` x s / 2 whatever the softness, and nothing a feather's half beyond it."""
    brightnesses = []
    for seed in range(8):
        speck = lone_speck(seed, size=40.0, softness=0.5)
        brightness = speck.max()
        brightnesses.append(brightness)
        assert 0.5 <= brightness <= 1.0
        rows, cols = np.mgrid[0:SIZE, 0:SIZE]
        centre = ((speck * rows).sum() / speck.sum(), (speck * cols).sum() / speck.sum())
        distance = np.hypot(rows - centre[0], cols - centre[1])
        radius = 20.0 * brightness
        assert speck[np.abs(distance - radius) < 0.5].mean() == pytest.approx(brightness / 2, abs=0.05 * brightness)
        assert speck[distance > 1.25 * radius + 1].max() == 0.0  # soft edge 0.5 radius wide: ends at 1.25 radii
    assert max(brightnesses) - min(brightnesses) > 0.2  # the depth differs from speck to speck


def test_a_crisp_speck_is_solid_to_half_a_pixel_inside_its_radius_and_ends_half_a_pixel_beyond_it(lone_speck) -> None:
    speck = lone_speck(2, size=40.0, softness=0.0)
    rows, cols = np.mgrid[0:SIZE, 0:SIZE]
    centre = ((speck * rows).sum() / speck.sum(), (speck * cols).sum() / speck.sum())
    distance = np.hypot(rows - centre[0], cols - centre[1])
    radius = 20.0 * speck.max()
    np.testing.assert_allclose(speck[distance < radius - 0.6], speck.max(), atol=1e-6)
    assert speck[distance > radius + 0.6].max() == 0.0


def test_a_softer_speck_has_a_wider_ramp_from_full_brightness_to_none(lone_speck) -> None:
    def ramp_pixels(softness: float) -> int:
        speck = lone_speck(2, size=40.0, softness=softness)
        return int(((speck > 0.05 * speck.max()) & (speck < 0.95 * speck.max())).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]


def test_a_bigger_size_makes_bigger_specks(lone_speck) -> None:
    small, large = (lone_speck(1, size=size, softness=0.0) for size in (12.0, 36.0))
    assert (large > 0).sum() > 6 * (small > 0).sum()  # discs: area grows with the square of the size, here 9 x
