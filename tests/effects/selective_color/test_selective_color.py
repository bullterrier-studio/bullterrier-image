"""The shipped presets of the `selective_color` effect; its behaviour is tested in Rust."""

import colorsys

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Step

NAME = "selective_color"


def pixel(hue_degrees: float, saturation: float = 1.0, value: float = 1.0) -> np.ndarray:
    """A 1x1 picture of the colour with this hue (0 red, 120 green, 240 blue), made with the standard library."""
    return np.array([[colorsys.hsv_to_rgb(hue_degrees / 360, saturation, value)]], np.float32)


def hue_of(picture: np.ndarray) -> float:
    """Hue in degrees of a 1x1 picture, by the standard library."""
    return colorsys.rgb_to_hsv(*map(float, picture[0, 0]))[0] * 360


def circular_distance(a: float, b: float) -> float:
    return abs((a - b + 180) % 360 - 180)


@pytest.mark.parametrize("hue", [0, 30, 90, 150, 200, 270, 330])
def test_the_hue_shift_preset_turns_a_vivid_colour_of_any_hue_150_degrees_round_the_wheel(hue: float) -> None:
    out = registry.apply(NAME, pixel(hue), **Step(NAME, preset="hue_shift").params)
    assert circular_distance(hue_of(out), hue + 150) < 0.05
    np.testing.assert_allclose(out.max(), 1.0, atol=1e-6)  # the brightness (HSV value) is kept
