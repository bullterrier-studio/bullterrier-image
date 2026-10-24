"""Tests of `ops/light.py`: the bright-pass and the screen blend the light effects share, against values worked out by
hand from their definitions."""

import numpy as np
import pytest

from photo_fx.ops.light import highlights, screen


@pytest.mark.parametrize(
    ("value", "threshold", "expected"),
    [
        pytest.param(0.5, 0.5, 0.0, id="a value at the threshold is not a highlight"),
        pytest.param(0.25, 0.5, 0.0, id="below the threshold nothing is kept"),
        pytest.param(1.0, 0.5, 1.0, id="white is a full highlight"),
        pytest.param(0.75, 0.5, 0.5, id="halfway between threshold and white is half: (0.75 - 0.5) / (1 - 0.5)"),
        pytest.param(0.9, 0.8, 0.5, id="the same halfway point for another threshold"),
        pytest.param(0.3, 0.0, 0.3, id="threshold 0 keeps the picture"),
    ],
)
def test_highlights_rise_from_zero_at_the_threshold_to_one_at_white(
    value: float, threshold: float, expected: float
) -> None:
    out = highlights(np.full((2, 3, 3), value, np.float32), threshold)
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_highlights_judge_each_channel_on_its_own() -> None:
    pixel = np.array([[[0.9, 0.5, 0.2]]], np.float32)
    np.testing.assert_allclose(highlights(pixel, 0.5), [[[0.8, 0.0, 0.0]]], atol=1e-6)


@pytest.mark.parametrize(
    ("base", "light", "expected"),
    [
        pytest.param(0.5, 0.0, 0.5, id="no light leaves the base"),
        pytest.param(0.5, 0.5, 0.75, id="1 - (1 - 0.5)(1 - 0.5)"),
        pytest.param(0.2, 0.6, 0.68, id="1 - 0.8 x 0.4"),
        pytest.param(0.0, 0.3, 0.3, id="over black the light is the result"),
        pytest.param(1.0, 0.3, 1.0, id="white stays white"),
        pytest.param(0.4, 1.0, 1.0, id="full light gives white"),
        pytest.param(0.4, 2.5, 1.0, id="light beyond 1 saturates instead of overshooting"),
        pytest.param(0.4, -0.5, 0.4, id="negative light is no light: screen never darkens"),
    ],
)
def test_screen_blends_light_over_the_base_as_one_minus_the_product_of_the_complements(
    base: float, light: float, expected: float
) -> None:
    out = screen(np.full((2, 2, 3), base, np.float32), np.full((2, 2, 3), light, np.float32))
    assert out.dtype == np.float32
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_screen_takes_a_light_per_channel() -> None:
    out = screen(np.full((1, 1, 3), 0.5, np.float32), np.array([[[0.0, 0.5, 1.0]]], np.float32))
    np.testing.assert_allclose(out, [[[0.5, 0.75, 1.0]]], atol=1e-6)


def test_no_light_leaves_every_value_exactly_as_it_is() -> None:
    base = np.random.default_rng(0).random((16, 16, 3), dtype=np.float32) * 0.3
    np.testing.assert_array_equal(screen(base, np.zeros_like(base)), base)
