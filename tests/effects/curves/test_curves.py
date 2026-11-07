"""The shipped presets of the `curves` effect; its behaviour is tested in Rust."""

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.recipe import Step

NAME = "curves"


def grey(*values: float) -> np.ndarray:
    """A 1 x n picture whose pixel i is the grey `values[i]`."""
    return np.repeat(np.array(values, np.float32)[None, :, None], 3, axis=2)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(0.0, 0.0, id="black stays black"),
        pytest.param(0.25, 0.25, id="the shadows are untouched"),
        pytest.param(0.5, 0.5, id="the curve peaks at the middle"),
        pytest.param(0.75, 0.25, id="the highlights fold back to the shadows' height"),
        pytest.param(1.0, 0.0, id="white goes to black"),
    ],
)
def test_the_solarize_preset_folds_the_highlights_back_down_at_its_points(value: float, expected: float) -> None:
    step = Step(NAME, preset="solarize")
    np.testing.assert_allclose(registry.apply(NAME, grey(value), **step.params), expected, atol=1e-6)
