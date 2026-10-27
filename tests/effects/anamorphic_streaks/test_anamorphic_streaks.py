"""Behaviour of the `anamorphic_streaks` effect."""

from typing import Any

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "anamorphic_streaks"


def flat(value: float, height: int = 9, width: int = 40) -> np.ndarray:
    return np.full((height, width, 3), value, np.float32)


def brute_force(rgb: np.ndarray, length: float, threshold: float, amount: float, color: list[float]) -> np.ndarray:
    """The effect pixel by pixel, from its definition.

    A channel's highlight is (value - threshold) / (1 - threshold) above the threshold, else 0. Each highlight sends
    light along its own row only, to a pixel d columns away with weight exp(-|d| / s) / Z, where s is `length` px
    and Z the sum of exp(-|d| / s) over every offset d that can occur in a row (-(w-1) to w-1), so the
    weights add up to 1; nothing enters from beyond the picture. The light, times `amount` and `color`, is screened
    over the picture: 1 - (1 - value)(1 - min(light, 1)).
    """
    height, width = rgb.shape[:2]
    scale = length
    z = sum(np.exp(-abs(d) / scale) for d in range(-(width - 1), width))
    out = np.empty_like(rgb, dtype=np.float64)
    for y in range(height):
        for x in range(width):
            for channel in range(3):
                light = 0.0
                for source in range(width):
                    highlight = max((rgb[y, source, channel] - threshold) / (1 - threshold), 0.0)
                    light += highlight * np.exp(-abs(x - source) / scale) / z
                shade = min(amount * color[channel] * light, 1.0)
                out[y, x, channel] = 1 - (1 - rgb[y, x, channel]) * (1 - shade)
    return out


def test_it_only_ever_brightens(photo: np.ndarray) -> None:
    assert (registry.apply(NAME, photo, threshold=0.5) >= photo - 1e-6).all()


def test_black_stays_black() -> None:
    np.testing.assert_array_equal(registry.apply(NAME, flat(0.0)), flat(0.0))


def test_a_threshold_above_every_value_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    dim = photo * 0.5
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5, amount=10.0), dim, atol=1e-6)


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param(
            {"length": 9.0, "threshold": 0.6, "amount": 4.0, "color": [1.0, 0.5, 0.25]},
            id="a quarter of the width, 9 px",
        ),
        pytest.param({"length": 1.0, "threshold": 0.5, "amount": 2.0, "color": [1.0, 1.0, 1.0]}, id="a short streak"),
        pytest.param(
            {"length": 37.0, "threshold": 0.7, "amount": 10.0, "color": [0.2, 0.4, 1.0]}, id="the whole width"
        ),
    ],
)
def test_the_picture_matches_a_pixel_by_pixel_computation(settings: dict[str, Any]) -> None:
    picture = np.random.default_rng(3).random((6, 37, 3), dtype=np.float32)  # values everywhere between 0 and 1
    expected = brute_force(picture, **settings)
    np.testing.assert_allclose(registry.apply(NAME, picture, **settings), expected, atol=5e-6)


def test_a_single_bright_pixel_spreads_along_its_row_only() -> None:
    picture = flat(0.0)
    picture[4, 12] = 1.0
    settings: dict[str, Any] = {"length": 8.0, "threshold": 0.5, "amount": 3.0, "color": [1.0, 0.5, 0.25]}
    out = registry.apply(NAME, picture, **settings)
    expected = brute_force(picture, **settings)
    np.testing.assert_allclose(out[4], expected[4], atol=5e-6)
    assert out[4, 30, 0] > 0  # far along the row
    other_rows = np.delete(out, 4, axis=0)
    np.testing.assert_array_equal(other_rows, np.zeros_like(other_rows))  # nothing leaks up or down


def test_no_light_comes_in_from_beyond_the_edge() -> None:
    picture = flat(0.0)
    picture[4, 0] = 1.0  # at the left edge
    out = registry.apply(NAME, picture, length=2.0, threshold=0.5, amount=10.0, color=[1.0, 1.0, 1.0])
    near, far = out[4, 1, 0], out[4, 39, 0]
    assert near > 0.1
    assert far < 1e-6  # the right edge, 39 px on, is far beyond a streak of 2 px; nothing wraps round to it
    assert out[4, 0, 0] == 1.0


def test_a_longer_streak_reaches_further_and_lights_less_close_by() -> None:
    picture = flat(0.0, 5, 81)
    picture[2, 40] = 1.0

    def row(length: float) -> np.ndarray:
        return registry.apply(NAME, picture, length=length, threshold=0.5, amount=1.0)[2, :, 0]

    short, long = row(4.0), row(32.0)
    assert long[70] > short[70]  # 30 px away: the long streak is brighter
    assert long[41] < short[41]  # next to the source the short one is


def test_the_streaks_keep_the_colour_of_the_light_when_the_colour_is_white() -> None:
    red_light = flat(0.0)
    red_light[4, 12] = [1.0, 0.0, 0.0]
    out = registry.apply(NAME, red_light, length=8.0, threshold=0.5, amount=3.0, color="#ffffff")
    assert out[4, 20, 0] > 0.01
    assert out[..., 1:].max() == 0.0


def test_color_tints_the_streaks() -> None:
    spot = flat(0.0)
    spot[4, 12] = 1.0
    r, g, b = registry.apply(NAME, spot, length=8.0, threshold=0.5, amount=3.0, color=[0.2, 0.5, 1.0])[4, 20]
    assert b > g > r > 0
    assert (g / b) == pytest.approx(0.5, rel=1e-4)
    assert (r / b) == pytest.approx(0.2, rel=1e-4)
