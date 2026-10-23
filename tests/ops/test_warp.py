"""Tests of `ops/warp.py`: reading a picture at float coordinates, and the coordinates of a warp about a centre."""

import math
from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.warp import Centre, remap

Interpolation = Literal["nearest", "bilinear", "bicubic"]
Edge = Literal["reflect", "mirror", "nearest", "wrap"]
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]
EDGES = ["reflect", "mirror", "nearest", "wrap"]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_reading_every_pixel_where_it_is_gives_the_picture_back(
    interpolation: Interpolation, edge: Edge, photo: np.ndarray
) -> None:
    rows, cols = np.mgrid[0:96, 0:128].astype(np.float64)
    out = remap(photo, rows, cols, interpolation=interpolation, edge=edge)
    assert out.dtype == np.float32
    np.testing.assert_allclose(out, photo, atol=1e-6)


def test_the_result_has_the_shape_of_the_coordinates_not_of_the_picture(photo: np.ndarray) -> None:
    rows, cols = np.mgrid[0:5, 0:7].astype(np.float64) * 3
    assert remap(photo, rows, cols, interpolation="bilinear", edge="reflect").shape == (5, 7, 3)


def test_nearest_reads_the_closest_pixel() -> None:
    index = grey(np.arange(10 * 10).reshape(10, 10) / 100.0)
    rows = np.array([[2.4, 2.6, 7.0]])
    cols = np.array([[5.6, 5.4, 0.4]])
    out = remap(index, rows, cols, interpolation="nearest", edge="reflect")[0, :, 0]
    np.testing.assert_allclose(out, [0.26, 0.35, 0.70], atol=1e-6)  # pixels (2, 6), (3, 5), (7, 0)


@pytest.mark.parametrize("interpolation", ["bilinear", "bicubic"])
def test_a_linear_picture_is_read_exactly_between_its_pixels(interpolation: Interpolation) -> None:
    """Whatever the smooth interpolation, a ramp 2 row + col is reproduced at any point: 2 row + col, worked out."""
    rows, cols = np.mgrid[0:41, 0:41].astype(np.float64)
    ramp = grey((2 * rows + cols) / 120.0)
    where_rows = np.array([[10.25, 20.5, 29.75]])
    where_cols = np.array([[15.5, 12.125, 22.0]])
    out = remap(ramp, where_rows, where_cols, interpolation=interpolation, edge="mirror")[0, :, 0]
    np.testing.assert_allclose(out, (2 * where_rows[0] + where_cols[0]) / 120.0, atol=1e-6)


def test_bicubic_reproduces_a_cubic_picture_between_its_pixels_where_bilinear_does_not() -> None:
    """A picture 0.5 + 0.5 ((column - 20) / 20)^3 over 41 columns is a cubic, which a cubic spline fits exactly away
    from the edge (columns 12 to 28 here) and straight lines between pixels do not."""
    cubic = grey(np.tile(0.5 + 0.5 * ((np.arange(41) - 20) / 20) ** 3, (3, 1)))
    cols = np.array([[12.3, 15.5, 19.9, 22.25, 27.7]])
    rows = np.ones_like(cols)
    truth = 0.5 + 0.5 * ((cols[0] - 20) / 20) ** 3
    errors = {
        choice: np.abs(remap(cubic, rows, cols, interpolation=choice, edge="mirror")[0, :, 0] - truth).max()
        for choice in ("bilinear", "bicubic")
    }
    assert errors["bicubic"] < 1e-6
    assert errors["bilinear"] > 1e-4


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.0), ("mirror", 0.15), ("reflect", 0.10), ("wrap", 0.90)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_lies_beyond_the_picture(edge: Edge, expected: float) -> None:
    """A picture of 21 rows with the value row / 20, read at row -3 (3 rows above the first): nearest repeats the first
    row (0); mirror reads row 3 (about row 0); reflect row 2 (about the edge of the first pixel, which it repeats);
    wrap row 18 (21 rows repeat)."""
    rows = grey(np.tile((np.arange(21) / 20.0)[:, None], (1, 5)))
    out = remap(rows, np.array([[-3.0]]), np.array([[2.0]]), interpolation="nearest", edge=edge)
    assert out[0, 0, 0] == pytest.approx(expected, abs=1e-6)


def test_a_smooth_interpolation_never_leaves_0_to_1_even_where_a_spline_overshoots() -> None:
    step = np.zeros((21, 21, 3), np.float32)
    step[:, 10:] = 1.0
    rows, cols = np.mgrid[0:61, 0:61].astype(np.float64) / 3
    out = remap(step, rows, cols, interpolation="bicubic", edge="reflect")
    assert out.min() >= 0.0
    assert out.max() <= 1.0
    assert out.min() == 0.0
    assert out.max() == 1.0


def test_the_picture_is_not_modified(photo: np.ndarray) -> None:
    before = photo.copy()
    rows, cols = np.mgrid[0:96, 0:128].astype(np.float64) * 0.9
    remap(photo, rows, cols, interpolation="bicubic", edge="wrap")
    np.testing.assert_array_equal(photo, before)


def test_the_distance_of_every_pixel_from_the_centre() -> None:
    """The middle of 31 rows by 61 columns is pixel (15, 30); the first pixel is hypot(15, 30) from it."""
    distance = Centre(31, 61, x=0.5, y=0.5).distance
    assert distance.shape == (31, 61)
    assert distance[15, 30] == 0.0
    assert distance[0, 0] == pytest.approx(math.sqrt(15**2 + 30**2))
    assert distance[15, 40] == pytest.approx(10.0)
    assert distance[0, 30] == pytest.approx(15.0)


@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.0, 0.0, (0, 0)), (1.0, 1.0, (30, 60)), (0.25, 0.8, (24, 15))],
    ids=["top left", "bottom right", "off centre"],
)
def test_x_and_y_are_fractions_of_the_way_from_the_first_pixel_to_the_last(
    x: float, y: float, pixel: tuple[int, int]
) -> None:
    distance = Centre(31, 61, x=x, y=y).distance
    assert distance[pixel] == 0.0
    assert (distance > 0).sum() == 31 * 61 - 1


def test_sources_with_nothing_asked_are_the_pixels_themselves() -> None:
    rows, cols = Centre(31, 61, x=0.3, y=0.6).sources()
    expected_rows, expected_cols = np.mgrid[0:31, 0:61]
    np.testing.assert_allclose(rows, expected_rows, atol=1e-9)
    np.testing.assert_allclose(cols, expected_cols, atol=1e-9)


def test_a_scale_moves_every_source_along_the_ray_through_the_centre() -> None:
    """Scale 2 puts the source of the first pixel (offset (-15, -30) from the centre (15, 30)) at (-15, -30): twice as
    far; scale 0.5 at (7.5, 15)."""
    centre = Centre(31, 61, x=0.5, y=0.5)
    rows, cols = centre.sources(scale=2.0)
    assert (rows[0, 0], cols[0, 0]) == pytest.approx((-15.0, -30.0))
    rows, cols = centre.sources(scale=0.5)
    assert (rows[0, 0], cols[0, 0]) == pytest.approx((7.5, 15.0))


def test_a_scale_may_differ_from_pixel_to_pixel() -> None:
    centre = Centre(31, 61, x=0.5, y=0.5)
    scale = np.where(centre.distance > 10, 2.0, 1.0)
    rows, cols = centre.sources(scale=scale)
    assert (rows[15, 38], cols[15, 38]) == pytest.approx((15.0, 38.0))  # 8 px away: kept
    assert (rows[15, 42], cols[15, 42]) == pytest.approx((15.0, 54.0))  # 12 px away: 24 px


def test_a_turn_reads_clockwise_from_the_source_of_each_pixel() -> None:
    """With a quarter turn the pixel 10 px right of the centre (15, 30) shows what is 10 px above it, the pixel 10 px
    below shows what is to its right (a picture turned clockwise moves its top to the right and its right to the
    bottom)."""
    centre = Centre(31, 61, x=0.5, y=0.5)
    rows, cols = centre.sources(turn=math.pi / 2)
    assert (rows[15, 40], cols[15, 40]) == pytest.approx((5.0, 30.0))
    assert (rows[25, 30], cols[25, 30]) == pytest.approx((15.0, 40.0))


def test_a_turn_and_a_scale_together_turn_the_scaled_offset() -> None:
    centre = Centre(31, 61, x=0.5, y=0.5)
    rows, cols = centre.sources(scale=2.0, turn=math.pi)
    assert (rows[15, 40], cols[15, 40]) == pytest.approx((15.0, 10.0))  # 10 px right: 20 px, half a turn, left
