"""Behaviour of the `fade` effect: lifting the blacks of a picture, worked out by hand."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "fade"


def grey(*values: float) -> np.ndarray:
    """A 1 x n picture whose pixel i is the grey `values[i]`."""
    return np.repeat(np.array(values, np.float32)[None, :, None], 3, axis=2)


@pytest.mark.parametrize(
    ("amount", "values", "expected"),
    [
        pytest.param(0.2, [0.0, 0.5, 1.0], [0.2, 0.6, 1.0], id="black rises to the amount, white stays, mid between"),
        pytest.param(0.5, [0.0, 0.25, 0.5], [0.5, 0.625, 0.75], id="half: every value moves halfway to white"),
        pytest.param(1.0, [0.0, 0.3, 1.0], [1.0, 1.0, 1.0], id="amount 1 lifts everything to white"),
        pytest.param(0.1, [1.0], [1.0], id="pure white is never dimmed"),
    ],
)
def test_known_values_are_lifted_as_worked_out_by_hand(
    amount: float, values: list[float], expected: list[float]
) -> None:
    out = registry.apply(NAME, grey(*values), amount=amount)
    np.testing.assert_allclose(out[0, :, 0], expected, atol=1e-6)


@pytest.mark.parametrize("amount", [0.05, 0.12, 0.4, 0.9])
def test_the_darkest_output_is_the_amount_and_no_pixel_gets_darker(photo: np.ndarray, amount: float) -> None:
    out = registry.apply(NAME, photo, amount=amount)
    assert out.min() >= amount - 1e-6
    assert np.all(out >= photo - 1e-6)
    assert out.max() <= 1.0  # white stays white, not 1.0000001, whatever the amount


def test_a_brighter_input_is_never_darker_in_the_output(photo: np.ndarray) -> None:
    ramp = np.sort(photo[..., 1].ravel())  # every green value of the photo, darkest first
    out = registry.apply(NAME, np.repeat(ramp[None, :, None], 3, axis=2), amount=0.3)[0, :, 1]
    assert np.all(np.diff(out) >= -1e-6)


def test_each_channel_is_lifted_on_its_own() -> None:
    pixel = np.array([[[0.0, 0.5, 1.0]]], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, pixel, amount=0.2)[0, 0], [0.2, 0.6, 1.0], atol=1e-6)
