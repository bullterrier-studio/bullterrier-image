"""Behaviour of the `fisheye` effect: the picture mapped through an equidistant fisheye lens, middle magnified, corners
kept."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "fisheye"
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (0.25, 0.75, (45, 20)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "the top left pixel", "off centre", "the right edge"],
)
def test_the_centre_pixel_does_not_move(interpolation: str, x: float, y: float, pixel: tuple[int, int]) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, amount=0.9, x=x, y=y, interpolation=interpolation)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is bent


@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
@pytest.mark.parametrize(
    ("x", "y", "corners"),
    [(0.5, 0.5, [(0, 0), (0, 80), (60, 0), (60, 80)]), (0.2, 0.9, [(0, 80)])],
    ids=["centred: all four corners are as far", "off centre: the one farthest corner"],
)
def test_the_corner_farthest_from_the_centre_stays_where_it_is(
    interpolation: str, x: float, y: float, corners: list[tuple[int, int]]
) -> None:
    """The lens is fitted so that the farthest corner, the edge of its field of view, is not moved. Off centre at
    (x, y) = (0.2, 0.9) the centre is pixel (54, 16) of 61 x 81 and the top right pixel is the farthest."""
    noise = np.random.default_rng(3).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, amount=0.7, x=x, y=y, interpolation=interpolation)
    for corner in corners:
        np.testing.assert_allclose(out[corner], noise[corner], atol=1e-5)
    assert np.abs(out - noise).max() > 0.05


@pytest.mark.parametrize(
    ("amount", "distance", "expected"),
    [(0.5, 35, 0.78963), (0.5, 20, 0.65972), (0.9, 35, 0.59428)],
    ids=["a half-amount lens, far out", "a half-amount lens, nearer", "a very wide lens"],
)
def test_a_pixel_shows_the_picture_at_the_distance_the_lens_maps_it_from(
    amount: float, distance: int, expected: float
) -> None:
    """The half field of view is amount x 90 degrees (0.5: 45 degrees). On a picture of 101 x 101 px the farthest
    corner is 50 sqrt(2) = 70.71 px from the centre (50, 50), and a pixel at distance d from it reads the picture at
    70.71 x tan(d / 70.71 x w) / tan(w), w the half angle: for the pixel 35 px right of the centre and amount 0.5,
    70.71 x tan(0.4950 x 45 degrees) / 1 = 28.96 px right of it, so a ramp column / 100 shows (50 + 28.96) / 100 =
    0.78963; likewise 0.65972 for 20 px, and with amount 0.9 (81 degrees) for 35 px, 0.59428."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), amount=amount, interpolation="bilinear")
    assert out[50, 50 + distance, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize(("x", "y"), [(0.5, 0.5), (0.2, 0.9)], ids=["centred", "off centre"])
@pytest.mark.parametrize("amount", [0.3, 0.8])
def test_every_pixel_shows_the_picture_at_the_distance_the_lens_maps_it_from(x: float, y: float, amount: float) -> None:
    """Brute force on a linear picture (read exactly between its pixels by bilinear interpolation): each pixel at
    distance d from the axis shows the ramp at R tan(d / R w) / tan(w) on the same ray, R the distance of the
    farthest of the four corners and w = amount x 90 degrees."""
    height, width = 31, 41
    axis_row, axis_col = y * (height - 1), x * (width - 1)
    farthest = max(math.hypot(r - axis_row, c - axis_col) for r in (0, height - 1) for c in (0, width - 1))
    w = math.radians(amount * 90)
    rows, cols = np.mgrid[0:height, 0:width].astype(np.float64)
    ramp = (2 * rows + cols + 30) / 150.0
    out = registry.apply(NAME, grey(ramp), amount=amount, x=x, y=y, interpolation="bilinear")[..., 0]
    expected = np.empty_like(ramp)
    for row in range(height):
        for col in range(width):
            d = math.hypot(row - axis_row, col - axis_col)
            scale = farthest * math.tan(d / farthest * w) / math.tan(w) / d if d > 0 else 1.0
            expected[row, col] = (
                2 * (axis_row + (row - axis_row) * scale) + axis_col + (col - axis_col) * scale + 30
            ) / 150.0
    np.testing.assert_allclose(out, expected, atol=1e-5)


def test_the_middle_is_magnified_and_nothing_is_read_from_farther_out_than_where_it_is_shown() -> None:
    """A picture whose value is the distance from the centre, as a fraction of the farthest corner's, comes out
    smaller everywhere (each pixel reads from nearer the centre), equal only at the centre and in the corners, and
    still rising along a row, so no part of the picture is folded over."""
    rows, cols = np.mgrid[0:101, 0:101].astype(np.float64) - 50
    distance = np.hypot(rows, cols) / (50 * math.sqrt(2))
    out = registry.apply(NAME, grey(distance), amount=0.7, interpolation="bilinear")[..., 0]
    inside = distance < 0.95
    assert (out[inside] < distance[inside] - 1e-3).sum() > 0.9 * inside.sum() - 100
    assert (out <= distance + 1e-4).all()
    assert (np.diff(out[50, 50:]) > 0).all()


@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_a_constant_picture_stays_as_it_is(interpolation: str) -> None:
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, amount=0.9, x=0.2, y=0.9, interpolation=interpolation)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_nearest_only_moves_whole_pixels_where_bilinear_blends_them() -> None:
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    nearest = registry.apply(NAME, noise, amount=0.6, interpolation="nearest")
    bilinear = registry.apply(NAME, noise, amount=0.6, interpolation="bilinear")
    assert np.isin(nearest, noise).all()
    assert not np.isin(bilinear, noise).all()


def test_bicubic_overshoot_is_clipped_to_the_range_of_a_picture() -> None:
    edge = np.zeros((41, 41, 3), np.float32)
    edge[:, 20:] = 1.0
    out = registry.apply(NAME, edge, amount=0.9, interpolation="bicubic")
    assert out.min() >= 0.0
    assert out.max() <= 1.0
