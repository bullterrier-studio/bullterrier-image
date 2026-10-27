"""Behaviour of the `channel_mixer` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "channel_mixer"
# The nine weights in row order: a row is one output channel, a column one input channel.
WEIGHTS = (
    "red_from_red",
    "red_from_green",
    "red_from_blue",
    "green_from_red",
    "green_from_green",
    "green_from_blue",
    "blue_from_red",
    "blue_from_green",
    "blue_from_blue",
)
TOLERANCE = 2 * float(np.finfo(np.float32).eps)  # 2 x float32 epsilon: float32 rounding of a sum of three products
MONOCHROME = registry.presets(NAME)["black_and_white"].params
PIXEL = np.array([[[0.2, 0.4, 0.6]]], np.float32)  # three distinct channels, so a swapped row or column shows


@pytest.mark.parametrize(
    ("matrix", "expected"),
    [
        pytest.param([1, 0, 0, 0, 1, 0, 0, 0, 1], [0.2, 0.4, 0.6], id="the identity matrix changes nothing"),
        pytest.param([0, 0, 1, 0, 1, 0, 1, 0, 0], [0.6, 0.4, 0.2], id="a permutation swaps red and blue"),
        pytest.param(
            [0.5, 0.25, 0.25, 0, 1, 0, 0, 0, 1],
            [0.35, 0.4, 0.6],  # red out = 0.5 * 0.2 + 0.25 * 0.4 + 0.25 * 0.6: a row is one output channel
            id="a row holds the shares of red, green and blue that make one output channel",
        ),
        pytest.param(
            [0, 0, 0, 1, 0, 0, 0, 0, 0],
            [0.0, 0.2, 0.0],  # green out = red in; red and blue out are empty rows
            id="the column of an input channel says where it goes",
        ),
        pytest.param(
            [2, 0, 0, 0, 1, 0, 0, 0, -1], [0.4, 0.4, 0.0], id="gain is allowed and a negative result clips to 0"
        ),
        pytest.param([3, 0, 0, 0, 1, 0, 0, 0, 1], [0.6, 0.4, 0.6], id="a gain of 3 on 0.2 lands at 0.6"),
        pytest.param([0, 0, 2, 0, 1, 0, 0, 0, 1], [1.0, 0.4, 0.6], id="a result above 1 clips to 1"),
    ],
)
def test_each_output_channel_is_the_row_of_the_matrix_applied_to_the_input_channels(
    matrix: list[float], expected: list[float]
) -> None:
    out = registry.apply(NAME, PIXEL, **dict(zip(WEIGHTS, matrix, strict=True)))
    np.testing.assert_allclose(out[0, 0], expected, atol=1e-6)


@pytest.mark.parametrize(
    ("pixel", "expected"),
    [
        # Rec. 709 luma weights (ITU-R BT.709), published as 0.2126 / 0.7152 / 0.0722.
        pytest.param((1, 0, 0), 0.2126, id="pure red has the luma of red"),
        pytest.param((0, 1, 0), 0.7152, id="pure green has the luma of green"),
        pytest.param((0, 0, 1), 0.0722, id="pure blue has the luma of blue"),
        pytest.param((0.2, 0.4, 0.6), 0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.6, id="a mixed colour"),
    ],
)
def test_the_black_and_white_preset_gives_every_channel_the_rec_709_luma(
    pixel: tuple[float, float, float], expected: float
) -> None:
    out = registry.apply(NAME, np.array([[pixel]], np.float32), **MONOCHROME)
    np.testing.assert_allclose(out[0, 0], [expected] * 3, atol=TOLERANCE)


def test_the_black_and_white_preset_keeps_greys_unchanged() -> None:
    ramp = np.linspace(0, 1, 21, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    out = registry.apply(NAME, ramp, **MONOCHROME)
    np.testing.assert_allclose(out, ramp, atol=TOLERANCE)
    assert out.max() <= 1.0  # white stays white: the shares sum to 1 and the result is clipped


def test_a_colour_filter_is_the_same_shares_in_every_row() -> None:
    # A red filter on black and white film, 0.9 : 0.4 : 0.1 as shares of the grey: each row holds the three shares.
    shares = {"red": 0.9 / 1.4, "green": 0.4 / 1.4, "blue": 0.1 / 1.4}
    rows = {f"{out}_from_{channel}": share for out in shares for channel, share in shares.items()}
    out = registry.apply(NAME, np.array([[(0.2, 0.4, 0.6)]], np.float32), **rows)
    np.testing.assert_allclose(out[0, 0], [(0.9 * 0.2 + 0.4 * 0.4 + 0.1 * 0.6) / 1.4] * 3, atol=TOLERANCE)


def test_the_default_leaves_a_photo_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)


@pytest.mark.parametrize("seed", [1, 2, 3])
def test_agrees_with_a_per_pixel_matrix_product_written_out_in_the_test(seed: int, photo: np.ndarray) -> None:
    matrix = np.random.default_rng(seed).uniform(-1, 1.5, size=(3, 3))
    small = photo[:6, :8]
    expected = np.zeros_like(small)
    for y in range(small.shape[0]):
        for x in range(small.shape[1]):
            for out_channel in range(3):
                total = sum(matrix[out_channel][c] * float(small[y, x, c]) for c in range(3))
                expected[y, x, out_channel] = min(max(total, 0.0), 1.0)
    out = registry.apply(NAME, small, **dict(zip(WEIGHTS, matrix.flatten().tolist(), strict=True)))
    np.testing.assert_allclose(out, expected, atol=1e-5)


@pytest.mark.parametrize(
    "matrix",
    [
        pytest.param([0.5, 0.25, 0.25, 0.2, 0.5, 0.3, 0.1, 0.1, 0.8], id="rows that sum to 1"),
        pytest.param([1 / 3] * 9, id="every share a third"),
    ],
)
def test_rows_summing_to_one_keep_greys_grey_and_unchanged(matrix: list[float]) -> None:
    ramp = np.linspace(0, 1, 11, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    np.testing.assert_allclose(registry.apply(NAME, ramp, **dict(zip(WEIGHTS, matrix, strict=True))), ramp, atol=1e-6)


@pytest.mark.parametrize("value", [-3.0, 3.0], ids=["every entry at the minimum", "every entry at the maximum"])
def test_extreme_weights_still_give_a_picture_in_range(value: float, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, **dict.fromkeys(WEIGHTS, value))
    assert np.isfinite(out).all()
    assert out.min() >= 0.0
    assert out.max() <= 1.0


@pytest.mark.parametrize(
    ("weight", "value"),
    [
        pytest.param("green_from_blue", 3.5, id="above the maximum"),
        pytest.param("blue_from_red", -3.5, id="below the minimum"),
        pytest.param("red_from_green", float("nan"), id="not a number"),
        pytest.param("red_from_black", 0.5, id="there is no such weight"),
    ],
)
def test_a_weight_that_is_out_of_range_or_unknown_is_refused(weight: str, value: float, photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match=weight):
        registry.apply(NAME, photo, **{weight: value})
