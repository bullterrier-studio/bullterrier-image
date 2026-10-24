"""Tests of `ops/ramp.py`: positions 0..1 looked up in evenly spaced colour stops, against values worked out by hand."""

import numpy as np
import pytest

from bullterrier_image.ops.ramp import ramp

RED, GREEN, BLUE = (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)


@pytest.mark.parametrize(
    ("stops", "t", "expected"),
    [
        pytest.param([RED, BLUE], 0.0, RED, id="0 is the first stop"),
        pytest.param([RED, BLUE], 1.0, BLUE, id="1 is the last stop"),
        pytest.param([RED, BLUE], 0.5, (0.5, 0.0, 0.5), id="halfway between two stops is their mean"),
        pytest.param([RED, BLUE], 0.25, (0.75, 0.0, 0.25), id="a quarter of the way is 3/4 first + 1/4 last"),
        pytest.param([RED, GREEN, BLUE], 0.5, GREEN, id="three stops sit at 0, 1/2 and 1"),
        pytest.param([RED, GREEN, BLUE], 0.25, (0.5, 0.5, 0.0), id="between the first two of three stops"),
        pytest.param([RED, GREEN, BLUE], 0.75, (0.0, 0.5, 0.5), id="between the last two of three stops"),
        pytest.param([RED, GREEN, BLUE, RED], 2 / 3, BLUE, id="four stops sit at 0, 1/3, 2/3 and 1"),
    ],
)
def test_ramp_interpolates_between_evenly_spaced_stops(stops: list, t: float, expected: tuple) -> None:
    out = ramp(np.array([t], np.float32), stops)
    np.testing.assert_allclose(out[0], expected, atol=1e-6)


def test_ramp_returns_float32_colours_with_a_trailing_channel_axis_for_any_shape_of_positions() -> None:
    t = np.linspace(0, 1, 24, dtype=np.float32).reshape(2, 3, 4)
    out = ramp(t, [RED, GREEN, BLUE])
    assert out.shape == (2, 3, 4, 3)
    assert out.dtype == np.float32
    np.testing.assert_array_equal(ramp(t[0], [RED, GREEN, BLUE]), out[0])


def test_ramp_does_not_modify_its_positions() -> None:
    t = np.linspace(0, 1, 7, dtype=np.float32)
    before = t.copy()
    ramp(t, [RED, BLUE])
    np.testing.assert_array_equal(t, before)
