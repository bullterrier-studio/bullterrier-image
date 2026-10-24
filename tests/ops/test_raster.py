"""Tests of `ops/raster.py`: which pixels a polygon covers, by the nonzero or the even-odd rule, and how much of each.

The oracles are written here: a brute-force winding number for every pixel centre, exact areas of rectangles and the
shoelace area of polygons.
"""

import math
import tracemalloc

import numpy as np
import pytest

from bullterrier_image.ops.raster import FillRule, coverage, paint

SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (150, 40)]  # the last is taller than the rows paint handles at a time


def winding(polygons: list[np.ndarray], x: float, y: float) -> tuple[int, int]:
    """(winding number, crossing count) of the closed polygons round the point, by walking every edge."""
    turns = crossings = 0
    for polygon in polygons:
        for (x0, y0), (x1, y1) in zip(polygon, np.roll(polygon, -1, axis=0), strict=True):
            if (y0 <= y) != (y1 <= y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
                turns += 1 if y1 > y0 else -1
                crossings += 1
    return turns, crossings


def oracle(polygons: list[np.ndarray], shape: tuple[int, int], rule: str) -> np.ndarray:
    """Whether each pixel's centre is inside, by the rule, from the brute-force winding number."""
    out = np.zeros(shape, bool)
    for row in range(shape[0]):
        for column in range(shape[1]):
            turns, crossings = winding(polygons, column + 0.5, row + 0.5)
            out[row, column] = turns != 0 if rule == "nonzero" else crossings % 2 == 1
    return out


def painted(polygons: list[np.ndarray], shape: tuple[int, int], rule: FillRule) -> np.ndarray:
    out = np.zeros(shape, bool)
    paint(out, True, polygons, rule=rule)
    return out


def shoelace(polygon: np.ndarray) -> float:
    x, y = polygon.T
    return abs(float(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))) / 2


def regular(sides: int, radius: float, centre: tuple[float, float], step: int = 1) -> np.ndarray:
    """A regular polygon (a star when `step` skips corners), corners every `step` of `sides` round the centre."""
    angle = 2 * math.pi * step * np.arange(sides) / sides
    return np.stack([centre[0] + radius * np.cos(angle), centre[1] + radius * np.sin(angle)], axis=1)


SHAPES = {
    "rectangle on pixel borders": [np.array([[1.0, 2], [5, 2], [5, 4], [1, 4]])],
    "triangle": [np.array([[0.3, 0.2], [9.7, 3.1], [2.2, 11.9]])],
    "pentagram": [regular(5, 9, (10.3, 10.1), step=2)],
    "thin sliver": [np.array([[0.0, 0.0], [12.0, 12.4], [12.0, 12.6]])],
    "two squares, the inner one the same way round": [
        np.array([[1.0, 1], [14, 1], [14, 14], [1, 14]]),
        np.array([[4.0, 4], [10, 4], [10, 10], [4, 10]]),
    ],
    "two squares, the inner one the other way round": [
        np.array([[1.0, 1], [14, 1], [14, 14], [1, 14]]),
        np.array([[4.0, 4], [4, 10], [10, 10], [10, 4]]),
    ],
    "running outside the picture": [np.array([[-30.0, -4], [40, 5.5], [8.5, 50]])],
    "wholly outside": [np.array([[-9.0, -9], [-2, -9], [-2, -2]])],
}


@pytest.mark.parametrize("rule", ["nonzero", "evenodd"])
@pytest.mark.parametrize("name", SHAPES)
def test_painted_pixels_are_those_whose_centre_is_inside_by_the_rule(name, rule):
    shape = (16, 18)
    np.testing.assert_array_equal(painted(SHAPES[name], shape, rule), oracle(SHAPES[name], shape, rule))


def test_the_rules_differ_where_a_shape_winds_twice():
    """A pentagram's middle winds twice: filled by the nonzero rule, a hole by the even-odd rule."""
    star = SHAPES["pentagram"]
    middle = (10, 10)
    assert painted(star, (21, 21), "nonzero")[middle]
    assert not painted(star, (21, 21), "evenodd")[middle]


def test_an_inner_outline_is_a_hole_only_when_it_runs_the_other_way_or_by_even_odd():
    same = SHAPES["two squares, the inner one the same way round"]
    opposite = SHAPES["two squares, the inner one the other way round"]
    assert painted(same, (16, 16), "nonzero")[7, 7]
    assert not painted(same, (16, 16), "evenodd")[7, 7]
    assert not painted(opposite, (16, 16), "nonzero")[7, 7]
    assert painted(opposite, (16, 16), "nonzero")[2, 2]


@pytest.mark.parametrize("shape", SIZES, ids=lambda s: f"{s[0]}x{s[1]}")
def test_any_picture_size_and_a_shape_taller_than_one_batch_of_rows(shape):
    height, width = shape
    polygons = [regular(7, 0.45 * max(shape), (width / 2.2, height / 1.9), step=3)]
    np.testing.assert_array_equal(painted(polygons, shape, "evenodd"), oracle(polygons, shape, "evenodd"))


def test_paint_writes_its_value_only_where_the_shape_is():
    labels = np.full((12, 12), 5, np.int32)
    paint(labels, 9, [np.array([[2.0, 2], [8, 2], [8, 8], [2, 8]])], rule="nonzero")
    assert set(np.unique(labels)) == {5, 9}
    assert (labels == 9).sum() == 36
    assert (labels[2:8, 2:8] == 9).all()


def test_no_polygon_and_one_without_area_paint_nothing():
    for polygons in (
        [],
        [np.array([[3.0, 3]])],
        [np.array([[1.0, 1], [6, 6]])],
        [np.array([[1.0, 4], [9, 4], [5, 4]])],
    ):
        assert not painted(polygons, (10, 10), "nonzero").any()


def test_a_polygon_is_closed_by_itself():
    open_square = np.array([[1.0, 1], [5, 1], [5, 5], [1, 5]])
    closed = np.vstack([open_square, open_square[:1]])
    np.testing.assert_array_equal(painted([open_square], (8, 8), "nonzero"), painted([closed], (8, 8), "nonzero"))


# ---- coverage ------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize("samples", [1, 4, 16])
def test_a_pixel_aligned_rectangle_is_covered_exactly(samples):
    out = coverage([np.array([[1.0, 2], [5, 2], [5, 4], [1, 4]])], (6, 8), rule="nonzero", samples=samples)
    want = np.zeros((6, 8), np.float32)
    want[2:4, 1:5] = 1
    np.testing.assert_allclose(out, want, atol=1e-6)


def test_a_rectangle_with_fractional_sides_covers_each_pixel_by_the_area_it_overlaps():
    left, right, top, bottom = 1.25, 4.5, 2.0, 5.0  # the vertical sides are on sub-sample boundaries for 4 samples
    out = coverage([np.array([[left, top], [right, top], [right, bottom], [left, bottom]])], (7, 7), "nonzero", 4)
    across = np.clip(np.minimum(np.arange(1, 8), right) - np.maximum(np.arange(7), left), 0, None)
    down = np.clip(np.minimum(np.arange(1, 8), bottom) - np.maximum(np.arange(7), top), 0, None)
    np.testing.assert_allclose(out, np.outer(down, across), atol=1e-6)


def test_the_coverage_of_a_circle_adds_up_to_its_area_and_stays_between_0_and_1():
    radius = 40.3
    circle = regular(720, radius, (50.2, 49.7))
    out = coverage([circle], (100, 101), rule="nonzero", samples=8)
    assert out.dtype == np.float32
    assert out.min() >= 0
    assert out.max() <= 1
    assert out.sum() == pytest.approx(math.pi * radius**2, rel=2e-3)
    assert ((out > 0) & (out < 1)).sum() < 4 * 2 * math.pi * radius  # only the border pixels are partial


def test_one_sample_looks_at_the_middle_of_each_pixel_row():
    """A rectangle whose top edge is at 2.3: the middle of row 2 (2.5) is inside, so with one sample row 2 is full."""
    out = coverage([np.array([[0.0, 2.3], [4, 2.3], [4, 5], [0, 5]])], (6, 4), "nonzero", 1)
    np.testing.assert_allclose(out[:, 0], [0, 0, 1, 1, 1, 0])


def test_coverage_follows_the_fill_rule():
    both = SHAPES["two squares, the inner one the same way round"]
    nonzero = coverage(both, (16, 16), "nonzero", 4)
    evenodd = coverage(both, (16, 16), "evenodd", 4)
    assert nonzero[7, 7] == 1
    assert evenodd[7, 7] == 0
    assert nonzero.sum() == pytest.approx(13 * 13)
    assert evenodd.sum() == pytest.approx(13 * 13 - 6 * 6)


def test_coverage_of_a_shape_running_outside_counts_only_the_part_inside():
    out = coverage([np.array([[-50.0, -50], [50, -50], [50, 3.5], [-50, 3.5]])], (10, 6), "nonzero", 2)
    np.testing.assert_allclose(out[:3], 1)
    np.testing.assert_allclose(out[3], 0.5)
    np.testing.assert_allclose(out[4:], 0)


def test_a_4k_picture_needs_little_more_than_its_own_result_and_no_supersampled_copy():
    shape = (2160, 3840)
    circle = regular(2000, 900, (1920, 1080))
    tracemalloc.start()
    out = coverage([circle], shape, "nonzero", 4)
    peak = tracemalloc.get_traced_memory()[1]
    tracemalloc.stop()
    assert out.sum() == pytest.approx(math.pi * 900**2, rel=1e-3)
    assert peak < 1.5 * out.nbytes
