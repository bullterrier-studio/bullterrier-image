"""Behaviour of the `curves` effect: tone curves through control points, worked out by hand."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "curves"
ANCHORS = (0.0, 0.25, 0.5, 0.75, 1.0)  # where the five master points sit on the input axis, as documented
MASTER_NAMES = ("black", "shadows", "midtones", "highlights", "white")


def grey(*values: float) -> np.ndarray:
    """A 1 x n picture whose pixel i is the grey `values[i]`."""
    return np.repeat(np.array(values, np.float32)[None, :, None], 3, axis=2)


def test_the_defaults_leave_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("settings", "values", "expected"),
    [
        # An S: shadows pulled down to 0.1, highlights up to 0.9. With the Fritsch-Carlson slopes worked out by hand
        # (chords 0.4, 1.6, 1.6, 0.4 per 0.25; middle slopes 0.64, 1.6, 0.64; the ends would point down, so 0) the
        # curve at the middle of a segment is 0.5 * (y0 + y1) + 0.03125 * (slope0 - slope1): 0.03, 0.27, then the
        # mirror image.
        pytest.param(
            {"shadows": 0.1, "highlights": 0.9},
            [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0],
            [0.0, 0.03, 0.1, 0.27, 0.5, 0.73, 0.9, 0.97, 1.0],
            id="an S-curve: points exact, between them the monotone cubic worked out by hand",
        ),
        pytest.param({"black": 0.2}, [0.0, 0.25, 1.0], [0.2, 0.25, 1.0], id="the black point lifts black only"),
        pytest.param({"white": 0.9}, [0.0, 0.75, 1.0], [0.0, 0.75, 0.9], id="the white point dims white only"),
        pytest.param(
            {"black": 1.0, "shadows": 0.75, "midtones": 0.5, "highlights": 0.25, "white": 0.0},
            [0.0, 0.25, 0.4, 1.0],
            [1.0, 0.75, 0.6, 0.0],
            id="points on a falling line make a negative",
        ),
    ],
)
def test_the_master_curve_maps_every_channel(
    settings: dict[str, float], values: list[float], expected: list[float]
) -> None:
    out = registry.apply(NAME, grey(*values), **settings)
    for channel in range(3):
        np.testing.assert_allclose(out[0, :, channel], expected, atol=1e-6)


@pytest.mark.parametrize("channel", ["red", "green", "blue"])
@pytest.mark.parametrize(
    ("point", "at", "height"), [("shadows", 0.25, 0.1), ("midtones", 0.5, 0.65), ("highlights", 0.75, 0.95)]
)
def test_each_point_of_a_channel_curve_moves_only_that_channel_at_that_input(
    channel: str, point: str, at: float, height: float
) -> None:
    out = registry.apply(NAME, grey(at), **{f"{channel}_{point}": height})
    expected = [at, at, at]
    expected["rgb".index(channel[0])] = height
    np.testing.assert_allclose(out[0, 0], expected, atol=1e-6)


@pytest.mark.parametrize(
    ("settings", "value", "expected"),
    [
        pytest.param({"red_midtones": 0.7}, 0.5, 0.7, id="red at its midpoint"),
        pytest.param(
            {"red_shadows": 0.1, "red_highlights": 0.9}, 0.375, 0.27, id="red follows the same cubic as the master"
        ),
        pytest.param({"red_highlights": 0.5}, 1.0, 1.0, id="the ends of a channel curve stay at black and white"),
        pytest.param({"red_shadows": 0.4}, 0.0, 0.0, id="the lower end of a channel curve stays at black"),
    ],
)
def test_a_channel_curve_is_a_curve_through_its_points(
    settings: dict[str, float], value: float, expected: float
) -> None:
    out = registry.apply(NAME, grey(value), **settings)
    np.testing.assert_allclose(out[0, 0, 0], expected, atol=1e-6)


def test_the_master_curve_is_applied_first_then_the_channel_curve() -> None:
    # master: 0.5 -> 0.25. Red: 0.25 -> 0.5 (its shadows point). In the other order red would be 0.5 -> 0.5 and then the
    # master would give 0.25.
    out = registry.apply(NAME, grey(0.5), midtones=0.25, red_shadows=0.5)
    np.testing.assert_allclose(out[0, 0], [0.5, 0.25, 0.25], atol=1e-6)


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param({"shadows": 0.02, "midtones": 0.9, "highlights": 0.95}, id="a steep step between flat stretches"),
        pytest.param({"black": 0.3, "shadows": 0.3, "midtones": 0.3}, id="a flat floor"),
        pytest.param({"midtones": 0.26, "highlights": 0.27}, id="an almost flat middle"),
    ],
)
def test_a_rising_master_curve_never_reorders_brightness_and_stays_in_range(
    photo: np.ndarray, settings: dict[str, float]
) -> None:
    ramp = np.sort(photo[..., 0].ravel())  # every red value of the photo, darkest first
    out = registry.apply(NAME, np.repeat(ramp[None, :, None], 3, axis=2), **settings)
    assert np.all(np.diff(out[0, :, 0]) >= -1e-6)
    assert out.min() >= 0.0
    assert out.max() <= 1.0


def test_a_curve_that_rises_and_falls_never_bulges_past_its_neighbouring_points() -> None:
    outputs = {"black": 0.2, "shadows": 0.9, "midtones": 0.3, "highlights": 0.4, "white": 0.35}
    sweep = np.linspace(0, 1, 1001, dtype=np.float32)
    out = registry.apply(NAME, grey(*sweep), **outputs)[0, :, 0]
    heights = list(outputs.values())
    for left in range(4):
        inside = (sweep >= ANCHORS[left]) & (sweep <= ANCHORS[left + 1])
        low, high = sorted(heights[left : left + 2])
        assert out[inside].min() >= low - 1e-6, MASTER_NAMES[left]  # an ordinary cubic through them would overshoot
        assert out[inside].max() <= high + 1e-6, MASTER_NAMES[left]
