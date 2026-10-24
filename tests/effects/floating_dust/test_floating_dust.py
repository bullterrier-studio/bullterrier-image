"""Behaviour of the `floating_dust` effect: faint, soft, out-of-focus specks of light."""

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "floating_dust"
SIZE = 160
WHITE = {"color": "#ffffff", "opacity": 1.0}


def black(size: int = SIZE) -> np.ndarray:
    return np.zeros((size, size, 3), np.float32)


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the specks add to a black picture, (h, w) in 0..1 (white specks at full opacity, so red is the light
    itself) unless `settings` say otherwise."""
    return registry.apply(NAME, black(size), **{**WHITE, **settings})[..., 0]


def lone_speck(seed: int, **settings: float) -> np.ndarray:
    """The light of the single speck of the first seed from `seed` on that is not cut off by the picture's edge."""
    for candidate in range(seed, seed + 50):
        speck = light({"count": 1, "seed": candidate, **settings})
        if speck[0].max() == 0 == speck[-1].max() == speck[:, 0].max() == speck[:, -1].max():
            return speck
    raise AssertionError("every one of 50 seeds put the speck on the picture's edge")


def test_no_specks_leave_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, count=0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("size", [200, 400])
def test_count_specks_are_made_whatever_the_size_of_the_picture(size: int) -> None:
    """The count is absolute, not per area: 12 small specks far apart are 12 separate patches of light."""
    mask = light({"count": 12, "size": 6.0, "softness": 0.0, "seed": 5}, size=size) > 0
    assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(12, abs=1)


def test_more_specks_touch_more_of_the_picture() -> None:
    fractions = [(light({"count": count, "seed": 4}) > 0).mean() for count in (5, 20, 80, 320)]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time


def test_a_speck_is_a_soft_disc_as_big_and_bright_as_its_depth() -> None:
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


def test_a_crisp_speck_is_solid_to_half_a_pixel_inside_its_radius_and_ends_half_a_pixel_beyond_it() -> None:
    speck = lone_speck(2, size=40.0, softness=0.0)
    rows, cols = np.mgrid[0:SIZE, 0:SIZE]
    centre = ((speck * rows).sum() / speck.sum(), (speck * cols).sum() / speck.sum())
    distance = np.hypot(rows - centre[0], cols - centre[1])
    radius = 20.0 * speck.max()
    np.testing.assert_allclose(speck[distance < radius - 0.6], speck.max(), atol=1e-6)
    assert speck[distance > radius + 0.6].max() == 0.0


def test_a_softer_speck_has_a_wider_ramp_from_full_brightness_to_none() -> None:
    def ramp_pixels(softness: float) -> int:
        speck = lone_speck(2, size=40.0, softness=softness)
        return int(((speck > 0.05 * speck.max()) & (speck < 0.95 * speck.max())).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]


def test_a_bigger_size_makes_bigger_specks() -> None:
    small, large = (lone_speck(1, size=size, softness=0.0) for size in (12.0, 36.0))
    assert (large > 0).sum() > 6 * (small > 0).sum()  # discs: area grows with the square of the size, here 9 x


def test_the_color_tints_the_light_and_the_opacity_scales_it() -> None:
    warm = registry.apply(NAME, black(), count=40, color="#ff8000", opacity=1.0, seed=1)
    assert warm[..., 0].max() > 0.5
    np.testing.assert_allclose(warm[..., 1], warm[..., 0] * 128 / 255, atol=0.003)  # green is 0x80 of red's 0xff
    assert warm[..., 2].max() == 0
    half = registry.apply(NAME, black(), count=40, color="#ff8000", opacity=0.5, seed=1)
    np.testing.assert_allclose(half, warm / 2, atol=1e-6)


def test_specks_are_screened_over_the_picture_so_they_only_lighten_it() -> None:
    """Screen over mid grey 0.5 with light l gives 1 - (1 - 0.5)(1 - l) = 0.5 + 0.5 l, and over white stays white."""
    grey = registry.apply(NAME, np.full((SIZE, SIZE, 3), 0.5, np.float32), count=60, seed=3, **WHITE)
    np.testing.assert_allclose(grey[..., 0], 0.5 + 0.5 * light({"count": 60, "seed": 3}), atol=1e-6)
    white = np.ones((SIZE, SIZE, 3), np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, white, count=60, seed=3, **WHITE), white)


@pytest.mark.parametrize("shape", [(60, 240), (240, 60)], ids=["wide", "tall"])
def test_the_specks_are_spread_over_the_whole_of_a_wide_or_a_tall_picture(shape: tuple[int, int]) -> None:
    lit = registry.apply(NAME, np.zeros((*shape, 3), np.float32), count=300, seed=1)[..., 0] > 0
    for axis in (0, 1):  # the picture's rows, then its columns: every third of them holds some specks
        thirds = np.array_split(lit.any(axis=1 - axis), 3)
        assert all(third.any() for third in thirds)


def test_the_same_seed_gives_the_same_specks_and_another_seed_other_specks(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert (registry.apply(NAME, photo, seed=1) != registry.apply(NAME, photo, seed=2)).any()
