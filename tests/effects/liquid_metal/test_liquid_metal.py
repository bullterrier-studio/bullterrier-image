"""Behaviour of the `liquid_metal` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "liquid_metal"
SIZE = 40
SLOPE = 0.005  # brightness gained per pixel along a test ramp
TINT = (0.8, 0.5, 0.2)


def ramp(direction: float) -> np.ndarray:
    """A grey picture of brightness 0.5 at its middle that rises by SLOPE per px towards `direction` (degrees
    clockwise from up: 180 = downwards, 90 = to the right)."""
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float64)
    grey = 0.5 + SLOPE * (x * np.sin(np.radians(direction)) - y * np.cos(np.radians(direction)))
    return np.repeat(grey[..., None], 3, axis=-1).astype(np.float32)


def test_relief_0_gives_a_flat_picture_of_the_tint_colour_whatever_the_picture(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, relief=0, horizon=0.5, tint=TINT)
    np.testing.assert_allclose(out, np.broadcast_to(TINT, out.shape), atol=1e-6)


def test_a_picture_of_the_same_brightness_in_other_hues_gives_the_same_chrome(photo: np.ndarray) -> None:
    """Adding (0.7152, -0.2126, 0) x a to a pixel leaves its Rec. 709 luminance (0.2126, 0.7152, 0.0722) unchanged."""
    grey = np.repeat(photo.mean(axis=-1, keepdims=True) * 0.5 + 0.25, 3, axis=-1)
    amount = np.random.default_rng(5).random(photo.shape[:2], dtype=np.float32)[..., None]  # a hue shift that varies
    hue = grey + 0.15 * amount * np.array([0.7152, -0.2126, 0.0], np.float32)
    assert np.abs(hue - grey).max() > 0.05
    kwargs = {"relief": 30, "smoothing": 1.5}
    np.testing.assert_allclose(registry.apply(NAME, hue, **kwargs), registry.apply(NAME, grey, **kwargs), atol=1e-4)


@pytest.mark.parametrize(
    ("direction", "relief", "horizon", "expected"),
    [
        pytest.param(180, 100, 0.5, (0.96, 0.9, 0.84), id="rising downwards: the reflection points up (t 0.9), light"),
        pytest.param(0, 100, 0.5, (0.16, 0.1, 0.04), id="rising upwards: the reflection points down (t 0.1), dark"),
        pytest.param(
            135,
            100,
            0.5,
            (0.91314, 0.78284, 0.65255),
            id="rising down and right: both slopes 0.354 lengthen the normal",
        ),
        pytest.param(90, 100, 0.5, TINT, id="rising sideways: the vertical environment sees no tilt, tint"),
        pytest.param(270, 100, 0.5, TINT, id="rising to the left: the same"),
        pytest.param(180, 100, 0.9, TINT, id="the horizon at the reflection's height: tint"),
        pytest.param(
            180, 100, 0.25, (0.97333, 0.93333, 0.89333), id="a lower horizon puts t 0.9 higher in the sky: lighter"
        ),
        pytest.param(0, 100, 0.25, (0.32, 0.2, 0.08), id="a lower horizon puts t 0.1 on the ground below it: dark"),
        pytest.param(
            180, 0, 0.25, (0.86667, 0.66667, 0.46667), id="a flat surface looks at t 0.5, above a lower horizon"
        ),
    ],
)
def test_on_a_ramp_the_colour_is_the_metal_gradient_at_the_height_the_reflection_points(
    direction: float, relief: float, horizon: float, expected: tuple[float, float, float]
) -> None:
    """A surface rising m = relief x SLOPE = 0.5 per px along the picture's down axis leans so that the viewer sees
    up 2m / (1 + m^2) = 0.8 of the way from straight down to straight up: t = 0.9. The metal runs black, `tint`, white
    as the environment runs from the ground (t 0) to the horizon to the sky (t 1)."""
    out = registry.apply(NAME, ramp(direction), relief=relief, horizon=horizon, tint=TINT, smoothing=0)
    np.testing.assert_allclose(out[5:-5, 5:-5], np.broadcast_to(expected, out[5:-5, 5:-5].shape), atol=1e-4)


@pytest.mark.parametrize(
    ("edge", "first_row_rise"),
    [
        pytest.param("reflect", 0.25, id="reflect repeats the edge pixel: half the slope"),
        pytest.param("nearest", 0.25, id="nearest repeats the edge pixel: half the slope"),
        pytest.param("mirror", 0.0, id="mirror reflects about the edge pixel: no slope"),
        pytest.param("wrap", -0.5 * (SIZE - 2) / 2, id="wrap sees the far side: a steep drop"),
    ],
)
def test_edge_decides_the_slope_in_the_first_row_and_nowhere_else(edge: str, first_row_rise: float) -> None:
    """With the horizon at 0.5 the environment is t itself; the metal then runs black, tint, white over t 0..1."""
    out = registry.apply(NAME, ramp(180), relief=100, horizon=0.5, tint=TINT, smoothing=0, edge=edge)
    t = 0.5 + first_row_rise / (1 + first_row_rise**2)
    expected = np.array(TINT) * min(2 * t, 1) + (1 - np.array(TINT)) * max(2 * t - 1, 0)
    np.testing.assert_allclose(out[0], np.broadcast_to(expected, out[0].shape), atol=1e-4)
    np.testing.assert_allclose(out[1:-1], np.broadcast_to((0.96, 0.9, 0.84), out[1:-1].shape), atol=1e-4)


def test_smoothing_calms_the_chrome_of_a_noisy_picture() -> None:
    noise = np.random.default_rng(3).random((64, 64, 3), dtype=np.float32)

    def variation(smoothing: float) -> float:
        return float(registry.apply(NAME, noise, relief=2, smoothing=smoothing)[..., 0].std())

    assert variation(0) > variation(1) > variation(3) > variation(8) > 0


def test_truncate_sets_how_far_into_the_picture_the_edge_rule_reaches_through_the_smoothing() -> None:
    """Smoothing of sigma 2 cut at `truncate` sigmas reads that many x 2 px beyond the border (plus 1 for the slope)."""
    rows = {}
    for truncate in (1.0, 4.0):
        reflect = registry.apply(NAME, ramp(180), relief=100, smoothing=2, truncate=truncate)
        wrap = registry.apply(NAME, ramp(180), relief=100, smoothing=2, truncate=truncate, edge="wrap")
        differing = np.flatnonzero(np.abs(reflect - wrap).max(axis=(1, 2)) > 1e-6)
        rows[truncate] = differing[differing < SIZE // 2]  # the top border's reach; the bottom one mirrors it
    assert rows[1.0].max() <= 3
    assert rows[4.0].max() >= 6
