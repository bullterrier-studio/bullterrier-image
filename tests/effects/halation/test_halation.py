"""Behaviour of the `halation` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "halation"


def flat(value: float, size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def test_halation_only_ever_brightens(photo: np.ndarray) -> None:
    assert (registry.apply(NAME, photo, radius=8.0) >= photo - 1e-6).all()


def test_black_stays_black() -> None:
    np.testing.assert_array_equal(registry.apply(NAME, flat(0.0)), flat(0.0))


def test_amount_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, amount=0.0), photo, atol=1e-6)


def test_a_threshold_above_every_value_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    dim = photo * 0.5
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5, amount=10.0), dim, atol=1e-6)


def test_a_single_bright_pixel_glows_as_a_gaussian_in_the_halation_colour() -> None:
    """The impulse response, worked out in the test: over black the screen adds the light as it is, so a white pixel
    becomes amount x colour x a 2-D Gaussian of sigma `radius` around it."""
    size, centre, radius, amount, colour = 61, 30, 4.0, 0.8, [1.0, 0.5, 0.25]
    picture = flat(0.0, size)
    picture[centre, centre] = 1.0
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    gaussian = np.exp(-squared / (2 * radius**2)) / (2 * np.pi * radius**2)
    out = registry.apply(NAME, picture, radius=radius, amount=amount, threshold=0.5, color=colour)
    mask = squared > 0
    for channel, weight in enumerate(colour):
        np.testing.assert_allclose(out[..., channel][mask], amount * weight * gaussian[mask], atol=2e-6)


def test_the_glow_is_redder_than_the_picture_it_lights() -> None:
    grey = flat(0.3, 61)
    grey[28:33, 28:33] = 1.0
    out = registry.apply(NAME, grey, radius=5.0, amount=1.0, threshold=0.5)  # default: a red-orange colour
    r, g, b = out[30, 40]  # 10 px from the spot, on the grey
    assert r > g > b > 0.3
    # the same glow in white would leave the grey neutral
    white = registry.apply(NAME, grey, radius=5.0, amount=1.0, threshold=0.5, color="#ffffff")[30, 40]
    assert white[1] == pytest.approx(white[0], abs=1e-6)
    assert white[2] == pytest.approx(white[0], abs=1e-6)
    assert white[0] > 0.3


def test_light_of_any_colour_makes_the_glow_not_only_its_own_channel() -> None:
    """A pure green highlight is as bright as it gets in its channel, so it glows, and in the halation colour."""
    green_light = flat(0.0, 41)
    green_light[18:23, 18:23, 1] = 1.0
    out = registry.apply(NAME, green_light, radius=4.0, amount=1.0, threshold=0.5, color="#ff0000")
    assert out[20, 27, 0] > 0.01  # red glow beside a green spot
    assert out[20, 27, 1:].max() == 0.0


def test_radius_sets_how_far_the_glow_spreads() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    far = (40, 52)  # 12 px away

    def light_at(radius: float) -> float:
        return float(registry.apply(NAME, dark, radius=radius, amount=1.0, threshold=0.5)[far][0])

    assert light_at(2.0) == 0  # a 2 px glow ends 4 x 2 = 8 px out
    assert light_at(6.0) > 0


def test_amount_sets_how_much_it_brightens(photo: np.ndarray) -> None:
    added = [float((registry.apply(NAME, photo, amount=a, radius=6.0) - photo).mean()) for a in (0.3, 0.9)]
    assert 0 < added[0] < added[1]


def test_truncate_sets_where_the_glow_is_cut_off() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    at_six_px = (40, 46)  # 3 sigma out for radius 2
    assert registry.apply(NAME, dark, radius=2.0, threshold=0.5, truncate=4.0)[at_six_px][0] > 0
    assert registry.apply(NAME, dark, radius=2.0, threshold=0.5, truncate=2.0)[at_six_px][0] == 0


def test_edge_sets_what_the_blur_sees_beyond_the_edge() -> None:
    lit_edge = flat(0.0, 41)
    lit_edge[20, 0] = 1.0
    far_side = (20, 40)
    assert registry.apply(NAME, lit_edge, radius=3.0, amount=1.0, threshold=0.5, edge="wrap")[far_side][0] > 0.01
    assert registry.apply(NAME, lit_edge, radius=3.0, amount=1.0, threshold=0.5, edge="reflect")[far_side][0] == 0.0


@pytest.mark.parametrize(
    ("threshold", "glows"),
    [
        pytest.param(0.5, True, id="a spot of 0.6 is above a threshold of 0.5"),
        pytest.param(0.65, False, id="below 0.65"),
    ],
)
def test_threshold_sets_the_brightness_where_the_glow_starts(threshold: float, glows: bool) -> None:
    dark = flat(0.0, 41)
    dark[20, 20] = 0.6
    out = registry.apply(NAME, dark, radius=3.0, amount=1.0, threshold=threshold)
    assert bool(out[20, 22, 0] > 0) is glows
