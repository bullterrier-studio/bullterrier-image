"""Tests of `ops/tone.py`: the tone remaps the colour effects share, against values worked out by hand."""

import numpy as np
import pytest

from photo_fx.ops.tone import remap_levels

ONE_PIXEL = np.array([[[0.3, 0.4, 0.6]]], np.float32)
IDENTITY = {"black": 0.0, "white": 1.0, "gamma": 1.0, "output_black": 0.0, "output_white": 1.0}


@pytest.mark.parametrize(
    ("settings", "expected"),
    [
        pytest.param(
            {"black": (0.1, 0.2, 0.3), "white": (0.5, 0.6, 0.9)},
            [0.5, 0.5, 0.5],
            id="each channel has its own points: 0.2/0.4, 0.2/0.4, 0.3/0.6",
        ),
        pytest.param(
            {"black": (0.1, 0.2, 0.3), "white": (0.5, 0.6, 0.9), "gamma": 2.0},
            [0.5**0.5] * 3,
            id="the gamma applies after the per-channel normalisation",
        ),
        pytest.param(
            {"black": 0.2, "white": (0.5, 0.6, 0.7)},
            [1 / 3, 0.5, 0.8],
            id="one black point is shared by every channel: 0.1/0.3, 0.2/0.4, 0.4/0.5",
        ),
        pytest.param(
            {"black": (0.0, 0.35, 0.0), "white": (1.0, 0.35, 1.0)},
            [0.3, 1.0, 0.6],
            id="a channel whose points are equal is cut at them; the others are untouched",
        ),
        pytest.param(
            {"black": 0.0, "white": 1.0, "output_black": (0.1, 0.2, 0.3), "output_white": (0.5, 0.6, 0.7)},
            [0.1 + 0.3 * 0.4, 0.2 + 0.4 * 0.4, 0.3 + 0.6 * 0.4],
            id="the output points can be per channel too",
        ),
    ],
)
def test_levels_are_remapped_with_points_given_per_channel_or_shared(
    settings: dict[str, object], expected: list[float]
) -> None:
    out = remap_levels(ONE_PIXEL, **{**IDENTITY, **settings})  # type: ignore[arg-type]
    np.testing.assert_allclose(out[0, 0], expected, atol=1e-6)


def test_the_result_is_a_new_float32_picture_of_the_same_shape_and_the_input_is_untouched(photo: np.ndarray) -> None:
    before = photo.copy()
    out = remap_levels(photo, **{**IDENTITY, "black": 0.2, "white": 0.7, "gamma": 1.3})
    assert out.shape == photo.shape
    assert out.dtype == np.float32
    assert out is not photo
    np.testing.assert_array_equal(photo, before)
