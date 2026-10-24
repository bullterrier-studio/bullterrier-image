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


def test_opacity_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, opacity=0.0), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("value", "opacity", "expected"),
    [
        pytest.param(0.4, 0.5, 0.52, id="0.4: screen with itself 0.64, halfway between 0.4 and 0.64"),
        pytest.param(0.4, 1.0, 0.64, id="opacity 1 is the full screen: 1 - 0.6 x 0.6"),
        pytest.param(0.4, 0.25, 0.46, id="a quarter of the way from 0.4 to 0.64"),
        pytest.param(1.0, 0.7, 1.0, id="white stays white"),
    ],
)
def test_an_even_picture_is_mixed_with_its_own_screen(value: float, opacity: float, expected: float) -> None:
    """A blur leaves an even picture as it is, so the blurred copy screened over it is 1 - (1 - v)^2."""
    np.testing.assert_allclose(registry.apply(NAME, flat(value), radius=5.0, opacity=opacity), expected, atol=1e-6)


def test_without_a_radius_every_value_is_mixed_with_its_own_screen(photo: np.ndarray) -> None:
    """Radius 0 blurs nothing: x goes to (1 - o) x + o (1 - (1 - x)^2), worked out per value."""
    opacity = 0.6
    expected = (1 - opacity) * photo + opacity * (1 - (1 - photo) ** 2)
    np.testing.assert_allclose(registry.apply(NAME, photo, radius=0.0, opacity=opacity), expected, atol=1e-6)


def test_a_single_bright_pixel_spreads_as_a_gaussian_of_the_radius() -> None:
    """The impulse response, worked out in the test: over black the screen adds the blurred copy as it is, so a pixel of
    0.5 becomes opacity x 0.5 x a 2-D Gaussian of sigma `radius` around it."""
    size, centre, radius, opacity = 61, 30, 4.0, 0.8
    picture = flat(0.0, size)
    picture[centre, centre] = 0.5
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    gaussian = np.exp(-squared / (2 * radius**2)) / (2 * np.pi * radius**2)
    out = registry.apply(NAME, picture, radius=radius, opacity=opacity)
    mask = squared > 0
    np.testing.assert_allclose(out[..., 0][mask], opacity * 0.5 * gaussian[mask], atol=2e-6)


def test_radius_sets_how_far_the_softness_spreads() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    far = (40, 52)  # 12 px away

    def light_at(radius: float) -> float:
        return float(registry.apply(NAME, dark, radius=radius, opacity=1.0)[far][0])

    assert light_at(2.0) == 0  # a 2 px blur ends 4 x 2 = 8 px out
    assert light_at(6.0) > 0


def test_opacity_sets_how_much_it_brightens(photo: np.ndarray) -> None:
    added = [float((registry.apply(NAME, photo, opacity=o, radius=6.0) - photo).mean()) for o in (0.3, 0.9)]
    assert 0 < added[0] < added[1]


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
    assert registry.apply(NAME, lit_edge, radius=3.0, opacity=1.0, edge="wrap")[far_side][0] > 0.01
    assert registry.apply(NAME, lit_edge, radius=3.0, opacity=1.0, edge="reflect")[far_side][0] == 0.0
