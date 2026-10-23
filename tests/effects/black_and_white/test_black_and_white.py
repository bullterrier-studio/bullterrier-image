"""Behaviour of the `black_and_white` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "black_and_white"


@pytest.mark.parametrize(
    ("pixel", "weights", "expected"),
    [
        # Rec. 709 luma weights (ITU-R BT.709): the default, published as 0.2126 / 0.7152 / 0.0722.
        pytest.param((1, 0, 0), {}, 0.2126, id="default: pure red has the Rec. 709 luma of red"),
        pytest.param((0, 1, 0), {}, 0.7152, id="default: pure green has the Rec. 709 luma of green"),
        pytest.param((0, 0, 1), {}, 0.0722, id="default: pure blue has the Rec. 709 luma of blue"),
        pytest.param((0.2, 0.4, 0.6), {}, 0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.6, id="default: a mixed colour"),
        pytest.param((0.2, 0.4, 0.6), {"red": 1, "green": 0, "blue": 0}, 0.2, id="a red filter keeps the red channel"),
        pytest.param((0.2, 0.4, 0.6), {"red": 0, "green": 1, "blue": 0}, 0.4, id="a green filter keeps the green"),
        pytest.param((0.2, 0.4, 0.6), {"red": 0, "green": 0, "blue": 1}, 0.6, id="a blue filter keeps the blue"),
        pytest.param((0.2, 0.4, 0.6), {"red": 1, "green": 1, "blue": 0}, 0.3, id="weights are shares: 1:1:0 averages"),
        pytest.param(
            (0.2, 0.4, 0.6), {"red": 0.6, "green": 0.2, "blue": 0}, 0.25, id="weights 3:1:0 are 75% red, 25% green"
        ),
        pytest.param((0.2, 0.4, 0.6), {"red": 0, "green": 0, "blue": 0}, 0.0, id="no light passes an all-zero filter"),
    ],
)
def test_the_grey_is_the_weighted_share_of_each_channel(
    pixel: tuple[float, float, float], weights: dict[str, float], expected: float
) -> None:
    out = registry.apply(NAME, np.array([[pixel]], np.float32), **weights)
    np.testing.assert_allclose(out[0, 0], [expected] * 3, atol=1e-6)


@pytest.mark.parametrize(
    "weights",
    [
        pytest.param({}, id="default weights"),
        pytest.param({"red": 1.0, "green": 0.0, "blue": 0.0}, id="a pure red filter"),
        pytest.param({"red": 0.3, "green": 0.1, "blue": 0.9}, id="uneven weights"),
        pytest.param({"red": 0.01, "green": 0.0, "blue": 0.0}, id="very small weights"),
    ],
)
def test_a_grey_picture_is_unchanged_whatever_the_weights(weights: dict[str, float]) -> None:
    ramp = np.linspace(0, 1, 21, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    np.testing.assert_allclose(registry.apply(NAME, ramp, **weights), ramp, atol=1e-6)


def test_only_the_ratio_of_the_weights_matters() -> None:
    rng = np.random.default_rng(7)
    picture = rng.random((5, 7, 3), dtype=np.float32)
    small = registry.apply(NAME, picture, red=0.1, green=0.2, blue=0.3)
    large = registry.apply(NAME, picture, red=0.2, green=0.4, blue=0.6)
    np.testing.assert_allclose(small, large, atol=1e-6)


def test_the_result_is_grey_and_agrees_with_a_per_pixel_sum_written_out_in_the_test(photo: np.ndarray) -> None:
    red, green, blue = 0.9, 0.3, 0.45
    small = photo[:6, :8]
    expected = np.zeros(small.shape[:2])
    for y in range(small.shape[0]):
        for x in range(small.shape[1]):
            r, g, b = (float(v) for v in small[y, x])
            expected[y, x] = (red * r + green * g + blue * b) / (red + green + blue)
    out = registry.apply(NAME, small, red=red, green=green, blue=blue)
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], expected, atol=1e-5)


@pytest.mark.parametrize("channel", [0, 1, 2], ids=["red", "green", "blue"])
def test_a_brighter_input_channel_never_gives_a_darker_grey(channel: int) -> None:
    levels = np.linspace(0, 1, 11, dtype=np.float32)
    picture = np.full((1, 11, 3), 0.3, np.float32)
    picture[0, :, channel] = levels
    grey = registry.apply(NAME, picture, red=0.5, green=0.7, blue=0.2)[0, :, 0]
    assert (np.diff(grey) > 0).all()  # every weight is positive here, so strictly brighter


@pytest.mark.parametrize(
    ("weights", "match"),
    [
        pytest.param({"red": 1.5}, "red", id="a weight above the maximum"),
        pytest.param({"green": -0.1}, "green", id="a negative weight"),
        pytest.param({"cyan": 1.0}, "cyan", id="a filter colour that does not exist"),
    ],
)
def test_a_weight_that_is_out_of_range_or_unknown_is_refused(weights: dict[str, float], match: str, photo) -> None:
    with pytest.raises(ValueError, match=match):
        registry.apply(NAME, photo, **weights)


@pytest.mark.parametrize(
    ("red", "green", "blue"),
    [(0.58, 0.3, 0.67), (0.41, 0.73, 0.71), (0.96, 0.37, 0.55)],
    ids=["weights A", "weights B", "weights C"],
)
def test_white_never_goes_above_one_even_when_the_normalised_weights_round_up(
    red: float, green: float, blue: float
) -> None:
    # In float32 the shares of these weights sum to slightly more than 1, so an unclipped white is 1.0000001.
    white = np.ones((2, 2, 3), np.float32)
    assert registry.apply(NAME, white, red=red, green=green, blue=blue).max() <= 1.0
