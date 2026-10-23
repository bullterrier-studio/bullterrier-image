"""Behaviour of the `invert` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "invert"


@pytest.mark.parametrize(
    ("pixel", "negative"),
    [
        pytest.param([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], id="black becomes white"),
        pytest.param([1.0, 1.0, 1.0], [0.0, 0.0, 0.0], id="white becomes black"),
        pytest.param([0.5, 0.5, 0.5], [0.5, 0.5, 0.5], id="mid grey stays where it is"),
        pytest.param([1.0, 0.0, 0.0], [0.0, 1.0, 1.0], id="red becomes its complement, cyan"),
        pytest.param([0.1, 0.5, 0.9], [0.9, 0.5, 0.1], id="each channel flips on its own"),
        pytest.param([0.25, 0.75, 0.2], [0.75, 0.25, 0.8], id="distinct values catch a swapped channel"),
    ],
)
def test_every_channel_value_v_becomes_one_minus_v(pixel: list[float], negative: list[float]) -> None:
    picture = np.array([[pixel]], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, picture)[0, 0], negative, atol=1e-6)


def test_inverting_twice_gives_back_the_picture(photo: np.ndarray) -> None:
    twice = registry.apply(NAME, registry.apply(NAME, photo))
    np.testing.assert_allclose(twice, photo, atol=1e-6)


def test_inverting_reverses_the_order_of_brightness_in_every_channel() -> None:
    ramp = np.linspace(0, 1, 11, dtype=np.float32)[None, :, None] * np.ones((1, 1, 3), np.float32)
    negative = registry.apply(NAME, ramp)
    assert (np.diff(negative, axis=1) < 0).all()  # strictly darker as the input brightens
