"""Tests of `ops/tone.py`: the tone remaps the colour effects share, against values worked out by hand."""

import numpy as np
import pytest

from photo_fx.ops.tone import monotone_curve, remap_levels

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


# --- monotone_curve ------------------------------------------------------------------------------------------------

KNEE = ((0.0, 0.5, 1.0), (0.0, 0.8, 1.0))  # a curve that bends sharply: a steep start, then almost flat


@pytest.mark.parametrize(
    ("points", "values", "expected"),
    [
        pytest.param(KNEE, [0.0, 0.5, 1.0], [0.0, 0.8, 1.0], id="a control point maps to its own output"),
        pytest.param(
            ((0.1, 0.3, 0.6, 0.9), (0.4, 0.2, 0.7, 0.65)),
            [0.1, 0.3, 0.6, 0.9],
            [0.4, 0.2, 0.7, 0.65],
            id="every control point of an uneven, non-monotone curve",
        ),
        # worked out by hand with the Fritsch-Carlson slopes: the middle slope is the harmonic mean of the two chords
        # (1.6 and 0.4), 0.64; the first end slope is the three-point estimate 2.2; the last would point down, so it
        # is 0
        pytest.param(KNEE, [0.25], [0.4975], id="between the first two points (cubic Hermite, t = 1/2)"),
        pytest.param(KNEE, [0.75], [0.94], id="between the last two points"),
        pytest.param(
            ((0.0, 1.0), (0.2, 0.6)), [0.0, 0.25, 0.5, 1.0], [0.2, 0.3, 0.4, 0.6], id="two points: a straight line"
        ),
        pytest.param(
            ((0.2, 0.8), (0.3, 0.9)), [0.0, 0.1, 0.9, 1.0], [0.3, 0.3, 0.9, 0.9], id="held flat beyond the end points"
        ),
        pytest.param(
            ((0.3, 0.5, 0.8), (0.4, 0.2, 0.9)),
            [0.0, 0.1, 0.9, 1.0],
            [0.4, 0.4, 0.9, 0.9],
            id="held flat beyond the end points of a curve that falls at its start (no extrapolated cubic)",
        ),
    ],
)
def test_monotone_curve_gives_the_values_worked_out_by_hand(
    points: tuple[tuple[float, ...], tuple[float, ...]], values: list[float], expected: list[float]
) -> None:
    out = monotone_curve(*points, np.array(values, np.float32))
    np.testing.assert_allclose(out, expected, atol=1e-6)


@pytest.mark.parametrize(
    "points",
    [
        pytest.param(((0.0, 0.4, 0.5, 1.0), (0.0, 0.1, 0.9, 1.0)), id="a steep step between two shallow stretches"),
        pytest.param(((0.0, 0.1, 0.9, 1.0), (0.0, 0.0, 1.0, 1.0)), id="flat, then a jump, then flat"),
        pytest.param(((0.0, 0.5, 1.0), (0.0, 0.8, 1.0)), id="an almost flat end"),
    ],
)
def test_a_rising_curve_never_reorders_brightness_and_never_leaves_its_outputs_range(
    points: tuple[tuple[float, ...], tuple[float, ...]],
) -> None:
    sweep = np.linspace(0, 1, 2001, dtype=np.float32)
    out = monotone_curve(*points, sweep)
    assert np.all(np.diff(out) >= -1e-6)  # brighter in, never darker out
    assert out.min() >= min(points[1])
    assert out.max() <= max(points[1])


def test_a_curve_that_rises_and_falls_stays_between_the_outputs_of_neighbouring_points() -> None:
    inputs, outputs = (0.0, 0.3, 0.6, 1.0), (0.2, 0.9, 0.3, 0.4)
    sweep = np.linspace(0, 1, 2001, dtype=np.float32)
    out = monotone_curve(inputs, outputs, sweep)
    for left in range(len(inputs) - 1):
        inside = (sweep >= inputs[left]) & (sweep <= inputs[left + 1])
        low, high = sorted(outputs[left : left + 2])
        assert out[inside].min() >= low - 1e-6, left  # a cubic through these points would dip or bulge past them
        assert out[inside].max() <= high + 1e-6, left


@pytest.mark.parametrize(
    "inputs",
    [
        pytest.param((0.0, 0.5, 0.5, 1.0), id="a repeated input"),
        pytest.param((0.0, 0.7, 0.5, 1.0), id="inputs out of order"),
    ],
)
def test_inputs_that_do_not_strictly_increase_are_refused(inputs: tuple[float, ...]) -> None:
    with pytest.raises(ValueError, match="increas"):
        monotone_curve(inputs, (0.0, 0.3, 0.6, 1.0), np.array([0.5], np.float32))


def test_a_curve_keeps_the_shape_of_the_values_it_is_given_as_float32(photo: np.ndarray) -> None:
    out = monotone_curve(*KNEE, photo)
    assert out.shape == photo.shape
    assert out.dtype == np.float32


@pytest.mark.parametrize("seed", range(60))
def test_a_curve_never_leaves_the_range_of_its_outputs_not_even_by_rounding(seed: int) -> None:
    rng = np.random.default_rng(seed)
    inputs = [0.0, *np.sort(rng.choice(np.arange(1, 100), size=4, replace=False) / 100), 1.0]
    outputs = rng.random(6)
    outputs[rng.integers(6)] = 0.0  # a point at pure black: a rounding error below it would be a negative value
    out = monotone_curve(inputs, outputs.tolist(), np.linspace(0, 1, 1001, dtype=np.float32))
    assert out.min() >= 0.0
    assert out.max() <= np.float32(outputs.max())
