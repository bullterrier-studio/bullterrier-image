"""Tests of `ops/light.py`: the screen blend the light effects share, against values worked out by hand from its
definition."""

import numpy as np
import pytest

from bullterrier_image.ops.light import screen


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
