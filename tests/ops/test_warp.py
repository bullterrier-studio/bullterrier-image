"""Tests of `ops/warp.py`: reading a picture at float coordinates."""

from typing import Literal

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.ops.warp import remap

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


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize(
    ("interpolation", "columns", "expected"),
    [
        ("bilinear", [99.5, 100.0, 100.5, -0.5, 250.5, -70.0], [0.495, 0.0, 0.005, 0.495, 0.505, 0.3]),
        ("nearest", [99.4, 99.6, 100.4, -0.4, 250.2, -70.3], [0.99, 0.0, 0.0, 0.0, 0.5, 0.3]),
    ],
    ids=["bilinear blends the last column with the first", "nearest takes the closest, the first beyond the last"],
)
def test_wrapped_columns_continue_the_picture_from_its_last_column_to_its_first(
    interpolation: Interpolation, columns: list[float], expected: list[float], edge: Edge
) -> None:
    """A picture of 100 columns with the value column / 100 (0 to 0.99): with `wrap_columns` the column after the last
    is the first, whatever the edge rule, which is for the rows. Halfway between the last and the first is
    (0.99 + 0) / 2 = 0.495 by 'bilinear'; at 100.5 it is (0 + 0.01) / 2 = 0.005; -0.5 is 99.5 again. A column
    however far, 250.5 or -70, is read modulo the width: 50.5 (0.505) or 30 (0.3)."""
    ramp = grey(np.tile(np.arange(100) / 100.0, (5, 1)))
    where = np.array([columns])
    out = remap(ramp, np.full_like(where, 2.0), where, interpolation=interpolation, edge=edge, wrap_columns=True)
    np.testing.assert_allclose(out[0, :, 0], expected, atol=1e-6)


def test_bicubic_carries_a_smooth_picture_across_the_seam_of_wrapped_columns() -> None:
    columns = np.arange(100)
    smooth = grey(np.tile(0.5 + 0.4 * np.sin(2 * np.pi * columns / 100), (5, 1)))
    where = np.array([[99.5, 100.25, 0.5, -0.75]])
    out = remap(smooth, np.full_like(where, 2.0), where, interpolation="bicubic", edge="reflect", wrap_columns=True)
    np.testing.assert_allclose(out[0, :, 0], 0.5 + 0.4 * np.sin(2 * np.pi * where[0] / 100), atol=2e-4)


def test_bicubic_across_the_seam_matches_a_spline_through_three_copies_side_by_side_on_noise() -> None:
    """Reference built in the test: the picture tiled three times along the columns, read at the same positions
    shifted by one picture width, where the seam lies far from the tiled picture's own ends."""
    noise = np.random.default_rng(8).random((6, 40, 3), dtype=np.float32)
    rows = np.full((1, 6), 2.5)
    cols = np.array([[39.5, 0.25, -0.5, 40.75, 20.5, 38.1]])
    tiled = np.tile(noise, (1, 3, 1))
    reference = np.stack(
        [ndimage.map_coordinates(tiled[..., c], np.stack([rows, cols + 40]), order=3, mode="mirror") for c in range(3)],
        axis=-1,
    )
    out = remap(noise, rows, cols, interpolation="bicubic", edge="mirror", wrap_columns=True)
    np.testing.assert_allclose(out, np.clip(reference, 0, 1), atol=1e-5)


def test_wrapped_columns_leave_the_edge_rule_to_the_rows() -> None:
    """Row -3 of a picture of 21 rows with the value row / 20 is, under 'nearest', the first row (0); row 5 is 0.25;
    the column 21 or 22 of 21 is the first or second, which no row value depends on."""
    values = grey(np.tile((np.arange(21) / 20.0)[:, None], (1, 21)))
    out = remap(
        values,
        np.array([[-3.0, 5.0]]),
        np.array([[21.0, 22.0]]),
        interpolation="nearest",
        edge="nearest",
        wrap_columns=True,
    )
    np.testing.assert_allclose(out[0, :, 0], [0.0, 0.25], atol=1e-6)


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
