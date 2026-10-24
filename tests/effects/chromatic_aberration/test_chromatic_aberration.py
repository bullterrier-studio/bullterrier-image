"""Behaviour of the `chromatic_aberration` effect: red and blue magnified differently from green, about a centre."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "chromatic_aberration"
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]
EDGES = ["reflect", "mirror", "nearest", "wrap"]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize(("x", "y"), [(0.5, 0.5), (0.1, 0.8)], ids=["centred", "off centre"])
def test_green_is_never_touched(interpolation: str, edge: str, x: float, y: float, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=12.0, x=x, y=y, interpolation=interpolation, edge=edge)
    np.testing.assert_array_equal(out[..., 1], photo[..., 1])
    assert np.abs(out[..., 0] - photo[..., 0]).max() > 0.05  # and red and blue are
    assert np.abs(out[..., 2] - photo[..., 2]).max() > 0.05


def test_red_moves_out_and_blue_moves_in_from_a_white_dot() -> None:
    """The axis on the left edge of 101 columns (x = 0, row 50): the farthest corner is hypot(50, 100) = 111.8 px
    away. With amount 22.36 (a fifth of that) a point d px from the axis has its red shifted outward by d / 5 and its
    blue inward by d / 5: red is read at 0.8 of the distance and blue at 1.2 of it. A white dot 60 px out has its
    red 75 px out (60 / 0.8), its green where it was and its blue 50 px out (60 / 1.2)."""
    dot = np.zeros((101, 101, 3), np.float32)
    dot[50, 60] = 1.0
    out = registry.apply(NAME, dot, amount=0.2 * math.hypot(50, 100), x=0.0, y=0.5, interpolation="nearest")
    assert np.argwhere(out[..., 0] > 0).tolist() == [[50, 75]]
    assert np.argwhere(out[..., 1] > 0).tolist() == [[50, 60]]
    assert np.argwhere(out[..., 2] > 0).tolist() == [[50, 50]]
    assert out.sum() == 3.0


def test_the_other_side_of_the_axis_is_shifted_the_other_way() -> None:
    """The axis on the right edge of 121 columns (x = 1, column 120), the farthest corner hypot(50, 120) = 130 px
    away, and the same fifth of it: a dot 60 px left of the axis has its red 75 px out, to the left, at column 45, and
    its blue 50 px out, at column 70."""
    dot = np.zeros((101, 121, 3), np.float32)
    dot[50, 60] = 1.0
    out = registry.apply(NAME, dot, amount=0.2 * math.hypot(50, 120), x=1.0, y=0.5, interpolation="nearest")
    assert np.argwhere(out[..., 0] > 0).tolist() == [[50, 45]]
    assert np.argwhere(out[..., 1] > 0).tolist() == [[50, 60]]
    assert np.argwhere(out[..., 2] > 0).tolist() == [[50, 70]]


@pytest.mark.parametrize(
    ("channel", "expected"),
    [(0, 0.86), (1, 0.90), (2, 0.94)],
    ids=["red, 0.9 of the way", "green, where it is", "blue, 1.1 of the way"],
)
def test_each_channel_shows_the_picture_at_its_own_scale_about_the_centre(channel: int, expected: float) -> None:
    """On 101 x 101 px the farthest corner is 70.71 px from the centre (50, 50); the amount 5 sqrt(2) = 7.07 px is a
    tenth of that, so red reads each point at 0.9 of its distance from the centre, green at 1, blue at 1.1. The pixel
    40 px right of the centre reads, on a ramp column / 100, column 50 + 36 = 86 (0.86), 90 and 94 (0.94)."""
    ramp = grey(np.tile(np.arange(101) / 100.0, (101, 1)))
    out = registry.apply(NAME, ramp, amount=5 * math.sqrt(2), interpolation="bilinear")
    assert out[50, 90, channel] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize(
    ("edge", "expected_blue"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_the_blue_sees_beyond_the_picture(edge: str, expected_blue: float) -> None:
    """A picture of 21 columns with the value column / 20, the centre on its top left pixel: the farthest corner is
    28.28 px away, so the amount 14.14 is a half of it and blue reads each point at 1.5 of its distance, red at
    0.5. The pixel (0, 20) reads blue at column 30, 10 beyond the last: nearest repeats the last column, 1.0; mirror
    reads column 10 (about column 20), 0.5; reflect column 11, 0.55; wrap column 9 (21 columns repeat), 0.45. Red reads
    column 10, 0.5, whatever the rule."""
    ramp = grey(np.tile(np.arange(21) / 20.0, (21, 1)))
    out = registry.apply(NAME, ramp, amount=math.hypot(20, 20) / 2, x=0.0, y=0.0, edge=edge, interpolation="nearest")
    assert out[0, 20, 2] == pytest.approx(expected_blue, abs=1e-6)
    assert out[0, 20, 0] == pytest.approx(0.5, abs=1e-6)


def test_a_shift_beyond_the_farthest_corner_is_limited_to_it() -> None:
    """The red cannot be pushed through the axis: a shift of the whole corner distance reads every red pixel at the
    axis, and a larger one reads the same."""
    noise = np.random.default_rng(3).random((31, 41, 3), dtype=np.float32)
    corner = math.hypot(15, 20)
    reach = registry.apply(NAME, noise, amount=corner, interpolation="nearest")
    beyond = registry.apply(NAME, noise, amount=corner * 3, interpolation="nearest")
    np.testing.assert_array_equal(beyond[..., 0], reach[..., 0])
    np.testing.assert_allclose(reach[..., 0], noise[15, 20, 0], atol=1e-6)


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_a_constant_picture_stays_as_it_is(edge: str, interpolation: str) -> None:
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, amount=20.0, x=0.2, y=0.9, edge=edge, interpolation=interpolation)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_nearest_only_moves_whole_pixels_where_bilinear_blends_them() -> None:
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    nearest = registry.apply(NAME, noise, amount=6.0, interpolation="nearest")
    bilinear = registry.apply(NAME, noise, amount=6.0, interpolation="bilinear")
    assert np.isin(nearest, noise).all()
    assert not np.isin(bilinear, noise).all()


def test_bicubic_overshoot_is_clipped_to_the_range_of_a_picture() -> None:
    step = np.zeros((41, 41, 3), np.float32)
    step[:, 20:] = 1.0
    out = registry.apply(NAME, step, amount=8.0, interpolation="bicubic")
    assert out.min() >= 0.0
    assert out.max() <= 1.0
