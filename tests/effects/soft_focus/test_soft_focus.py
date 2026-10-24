"""Behaviour of the `soft_focus` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "soft_focus"


def flat(value: float, size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def test_soft_focus_only_ever_brightens(photo: np.ndarray) -> None:
    assert (registry.apply(NAME, photo, radius=6.0) >= photo - 1e-6).all()


def test_black_stays_black() -> None:
    np.testing.assert_array_equal(registry.apply(NAME, flat(0.0)), flat(0.0))


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(0.4, 0.64, id="0.4 screened with itself: 1 - 0.6 x 0.6"),
        pytest.param(0.5, 0.75, id="0.5 screened with itself: 1 - 0.5 x 0.5"),
        pytest.param(1.0, 1.0, id="white stays white"),
    ],
)
def test_an_even_picture_becomes_its_own_screen(value: float, expected: float) -> None:
    """A blur leaves an even picture as it is, so the blurred copy screened over it is 1 - (1 - v)^2."""
    np.testing.assert_allclose(registry.apply(NAME, flat(value), radius=5.0), expected, atol=1e-6)


def test_without_a_radius_every_value_is_screened_with_itself(photo: np.ndarray) -> None:
    """Radius 0 blurs nothing: x goes to 1 - (1 - x)^2, worked out per value."""
    np.testing.assert_allclose(registry.apply(NAME, photo, radius=0.0), 1 - (1 - photo) ** 2, atol=1e-6)


def test_a_single_bright_pixel_spreads_as_a_gaussian_of_the_radius() -> None:
    """The impulse response, worked out in the test: over black the screen adds the blurred copy as it is, so a pixel of
    0.5 becomes 0.5 x a 2-D Gaussian of sigma `radius` around it."""
    size, centre, radius = 61, 30, 4.0
    picture = flat(0.0, size)
    picture[centre, centre] = 0.5
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    gaussian = np.exp(-squared / (2 * radius**2)) / (2 * np.pi * radius**2)
    out = registry.apply(NAME, picture, radius=radius)
    mask = squared > 0
    np.testing.assert_allclose(out[..., 0][mask], 0.5 * gaussian[mask], atol=2e-6)


def test_radius_sets_how_far_the_softness_spreads() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    far = (40, 52)  # 12 px away

    def light_at(radius: float) -> float:
        return float(registry.apply(NAME, dark, radius=radius)[far][0])

    assert light_at(2.0) == 0  # a 2 px blur ends 4 x 2 = 8 px out
    assert light_at(6.0) > 0


def test_truncate_sets_where_the_blur_is_cut_off() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    at_six_px = (40, 46)  # 3 sigma out for radius 2
    assert registry.apply(NAME, dark, radius=2.0, truncate=4.0)[at_six_px][0] > 0
    assert registry.apply(NAME, dark, radius=2.0, truncate=2.0)[at_six_px][0] == 0


def test_edge_sets_what_the_blur_sees_beyond_the_edge() -> None:
    lit_edge = flat(0.0, 41)
    lit_edge[20, 0] = 1.0
    far_side = (20, 40)
    assert registry.apply(NAME, lit_edge, radius=3.0, edge="wrap")[far_side][0] > 0.01
    assert registry.apply(NAME, lit_edge, radius=3.0, edge="reflect")[far_side][0] == 0.0
