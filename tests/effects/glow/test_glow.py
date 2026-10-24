"""Behaviour of the `glow` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "glow"


def flat(value: float, size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def screen(base: float, light: float) -> float:
    """The screen blend mode, from its definition: 1 - (1 - a)(1 - b)."""
    return 1.0 - (1.0 - base) * (1.0 - light)


def test_glow_only_ever_brightens(photo: np.ndarray) -> None:
    assert (registry.apply(NAME, photo) >= photo - 1e-6).all()


def test_amount_sets_how_much_it_brightens(photo: np.ndarray) -> None:
    added = [float((registry.apply(NAME, photo, amount=a, radius=6.0) - photo).mean()) for a in (0.3, 0.9)]
    assert 0 < added[0] < added[1]


def test_a_picture_darker_than_the_threshold_does_not_glow(photo: np.ndarray) -> None:
    dim = photo * 0.5  # every value below 0.5
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5, amount=1.0), dim, atol=1e-6)  # float rounding


@pytest.mark.parametrize(
    ("value", "amount", "threshold"), [(0.8, 0.6, 0.55), (0.9, 0.3, 0.2), (0.7, 2.0, 0.6), (0.9, 3.0, 0.5)]
)
def test_an_even_picture_is_screened_with_its_own_highlight(value: float, amount: float, threshold: float) -> None:
    """A blur leaves an even picture as it is, so the halo is the highlight: (value - threshold) / (1 - threshold)."""
    highlight = (value - threshold) / (1 - threshold)
    expected = screen(value, min(amount * highlight, 1.0))
    out = registry.apply(NAME, flat(value), radius=5.0, amount=amount, threshold=threshold)
    np.testing.assert_allclose(out, expected, atol=1e-5)


def test_the_glow_keeps_the_colour_of_the_light() -> None:
    red_light = flat(0.0, 41)
    red_light[18:23, 18:23, 0] = 1.0
    out = registry.apply(NAME, red_light, radius=4.0, amount=1.0)
    assert out[20, 27, 0] > 0.05  # the red spills beyond the spot
    assert out[..., 1:].max() == 0.0  # and nothing else does


def test_radius_sets_how_far_the_halo_spreads() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0  # one bright pixel
    near, far = (40, 44), (40, 52)  # 4 and 12 px away

    def light_at(radius: float, where: tuple[int, int]) -> float:
        return float(registry.apply(NAME, dark, radius=radius, amount=1.0, threshold=0.5)[where][0])

    assert light_at(2.0, near) > 0
    assert light_at(2.0, far) == 0  # a 2 px halo ends 4 x 2 = 8 px out
    assert light_at(6.0, far) > 0


def test_truncate_sets_where_the_halo_is_cut_off() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    at_six_px = (40, 46)  # 3 sigma out for radius 2
    assert registry.apply(NAME, dark, radius=2.0, threshold=0.5, truncate=4.0)[at_six_px][0] > 0
    assert registry.apply(NAME, dark, radius=2.0, threshold=0.5, truncate=2.0)[at_six_px][0] == 0


def test_mode_constant_lets_the_glow_fade_toward_the_edge_and_reflect_does_not() -> None:
    bright = flat(0.8)
    reflected = registry.apply(NAME, bright, radius=4.0, edge="reflect")
    np.testing.assert_allclose(reflected[0, 0], reflected[16, 16], atol=1e-6)  # an even picture stays even
    dark_beyond = registry.apply(NAME, bright, radius=4.0, edge="constant", constant_value=0.0)
    lit_beyond = registry.apply(NAME, bright, radius=4.0, edge="constant", constant_value=1.0)
    assert dark_beyond[0, 0, 0] < dark_beyond[16, 16, 0] - 0.01
    assert lit_beyond[0, 0, 0] > dark_beyond[0, 0, 0]
