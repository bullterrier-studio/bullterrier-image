"""Behaviour of the `bump_light` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "bump_light"
SIZE = 40
SLOPE = 0.005  # brightness gained per pixel along a test ramp


def ramp(direction: float) -> np.ndarray:
    """A grey picture of brightness 0.5 at its middle that rises by SLOPE per px towards `direction` (degrees
    clockwise from up)."""
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float64)
    grey = 0.5 + SLOPE * (x * np.sin(np.radians(direction)) - y * np.cos(np.radians(direction)))
    return np.repeat(grey[..., None], 3, axis=-1).astype(np.float32)


def lambert_gain(rise_toward_light: float, steepness: float, elevation: float) -> float:
    """What a surface of the given steepness (slope per px, either way) that rises `rise_toward_light` per px towards
    the light does to the picture, worked out by hand: the unit normal (leaning away from the rise) dotted with the
    light's direction, in proportion to what a flat surface gets (sin of the elevation)."""
    up = np.sin(np.radians(elevation))
    across = np.cos(np.radians(elevation))
    return float((up - across * rise_toward_light) / np.sqrt(1 + steepness**2) / up)


def test_height_0_leaves_the_picture_unchanged_and_so_does_a_flat_one(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, height=0, angle=100, elevation=20), photo, atol=1e-6)
    flat = np.full((SIZE, SIZE, 3), (0.9, 0.2, 0.4), np.float32)
    np.testing.assert_allclose(registry.apply(NAME, flat, height=30, blur=2), flat, atol=1e-6)


@pytest.mark.parametrize(
    ("light", "direction", "height", "elevation"),
    [
        pytest.param(270, 90, 20.0, 45.0, id="a slope facing the light (rises away from it) is brighter"),
        pytest.param(90, 90, 20.0, 45.0, id="the same slope lit from the other side is darker"),
        pytest.param(0, 90, 20.0, 45.0, id="light across the slope: only the normal's tilt dims it"),
        pytest.param(315, 135, 40.0, 30.0, id="diagonal ramp, diagonal light, low sun"),
        pytest.param(100, 20, 40.0, 70.0, id="arbitrary angles, high sun"),
        pytest.param(10, 200, 40.0, 90.0, id="light from straight above: the angle no longer matters"),
    ],
)
def test_on_a_ramp_the_picture_is_scaled_by_the_lambert_term_worked_out_by_hand(
    light: float, direction: float, height: float, elevation: float
) -> None:
    picture = ramp(direction)
    out = registry.apply(NAME, picture, angle=light, elevation=elevation, height=height, blur=0)
    rise = height * SLOPE
    gain = lambert_gain(rise * np.cos(np.radians(light - direction)), rise, elevation)
    inside = (slice(5, -5), slice(5, -5))
    np.testing.assert_allclose(out[inside], picture[inside] * gain, atol=2e-4)


def test_the_slope_facing_the_light_is_brighter_and_the_one_facing_away_darker_by_the_same_step() -> None:
    """A normal's tilt towards the light adds what the same tilt away takes off (before the normal's length: the two
    differ from 1 by gains that sum to 2 / sqrt(1 + slope^2))."""
    rising_away = registry.apply(NAME, ramp(90), angle=270, height=20, elevation=45)[10:-10, 10:-10]
    rising_toward = registry.apply(NAME, ramp(90), angle=90, height=20, elevation=45)[10:-10, 10:-10]
    picture = ramp(90)[10:-10, 10:-10]
    assert (rising_away > picture).all()
    assert (rising_toward < picture).all()
    steepness = 20 * SLOPE
    np.testing.assert_allclose((rising_away + rising_toward) / picture, 2 / np.sqrt(1 + steepness**2), atol=2e-4)


def test_the_lighting_is_a_grey_gain_so_colour_ratios_are_kept(photo: np.ndarray) -> None:
    dim = photo * 0.5  # headroom, so that no channel is clipped
    out = registry.apply(NAME, dim, height=10, elevation=45)
    assert not np.allclose(out, dim, atol=0.01)  # it did light something
    np.testing.assert_allclose(out[..., 0] / dim[..., 0], out[..., 1] / dim[..., 1], atol=1e-4)
    np.testing.assert_allclose(out[..., 2] / dim[..., 2], out[..., 1] / dim[..., 1], atol=1e-4)


def test_blur_smooths_the_height_so_the_lighting_varies_less(photo: np.ndarray) -> None:
    def variation(blur: float) -> float:
        out = registry.apply(NAME, photo * 0.5, height=10, blur=blur)
        return float((out / (photo * 0.5)).std())

    assert variation(0) > variation(1) > variation(3) > 0


@pytest.mark.parametrize(
    ("edge", "first_column_rise"),
    [
        pytest.param("reflect", SLOPE / 2, id="reflect repeats the edge pixel: half the slope"),
        pytest.param("nearest", SLOPE / 2, id="nearest repeats the edge pixel: half the slope"),
        pytest.param("mirror", 0.0, id="mirror reflects about the edge pixel: no slope"),
        pytest.param("wrap", -SLOPE * (SIZE - 2) / 2, id="wrap sees the far side: a steep drop"),
    ],
)
def test_edge_decides_the_slope_in_the_first_column_and_nowhere_else(edge: str, first_column_rise: float) -> None:
    height, picture = 4, ramp(90)  # gentle enough that even the wrapped column is not clipped to black
    out = registry.apply(NAME, picture, angle=270, elevation=45, height=height, blur=0, edge=edge)
    for column, rise in [(0, first_column_rise), (1, SLOPE), (SIZE // 2, SLOPE)]:
        gain = lambert_gain(-height * rise, height * rise, 45)  # light from the left: rising right is away from it
        np.testing.assert_allclose(out[:, column], picture[:, column] * gain, atol=2e-4)


def test_truncate_sets_how_far_into_the_picture_the_edge_rule_reaches_through_the_blur() -> None:
    """A blur of sigma 2 cut at `truncate` sigmas reads that many x 2 px beyond the border (plus 1 for the slope)."""
    picture = ramp(90)
    columns = {}
    for truncate in (1.0, 4.0):
        reflect = registry.apply(NAME, picture, angle=270, height=20, blur=2, truncate=truncate, edge="reflect")
        wrap = registry.apply(NAME, picture, angle=270, height=20, blur=2, truncate=truncate, edge="wrap")
        differing = np.flatnonzero(np.abs(reflect - wrap).max(axis=(0, 2)) > 1e-6)
        columns[truncate] = differing[differing < SIZE // 2]  # the left border's reach; the right one mirrors it
    assert columns[1.0].max() <= 3  # the blur reaches 2 px, the slope 1 more
    assert columns[4.0].max() >= 6  # a blur cut at 4 sigma reaches 8 px, its tail beyond 2 sigma small but not nil


def test_a_grazing_light_clips_the_lit_side_to_white_and_the_far_side_to_black(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, height=100, elevation=1)
    assert out.max() == 1.0
    assert out.min() == 0.0
    assert (out == 1.0).mean() > 0.01
    assert (out == 0.0).mean() > 0.01
