"""Tests of `ops/particles.py`: soft discs and segments drawn onto a layer of light."""

import numpy as np
import pytest

from photo_fx.ops.particles import draw_segments

SHAPE = (40, 60)


def one(
    start: tuple[float, float], end: tuple[float, float], *, radius: float, weight: float = 1.0, feather: float = 1.0
):
    """The layer of one segment from `start` to `end` (x, y)."""
    return draw_segments(SHAPE, np.array([start]), np.array([end]), radius, weight, feather)


@pytest.mark.parametrize(
    ("row", "column", "expected"),
    [
        (20, 30, 1.0),  # on the segment
        (21, 30, 1.0),  # 1 px off it, inside the radius of 2
        (22, 30, 0.5),  # exactly the radius away: half covered
        (23, 30, 0.0),  # 3 px away, beyond radius + half a pixel
        (20, 12, 1.0),  # 1 px short of its start: inside, the end is round
        (20, 8, 0.5),  # 2 px short of the start, the radius away from its end point
        (20, 52, 0.5),  # 2 px beyond its end
        (22, 8, 0.0),  # a corner: 2.83 px from the end point
    ],
)
def test_a_segment_covers_the_pixels_within_its_radius_and_half_covers_the_ring_at_the_radius(
    row: int, column: int, expected: float
) -> None:
    layer = one((10.0, 20.0), (50.0, 20.0), radius=2.0)
    assert layer[row, column] == pytest.approx(expected)


@pytest.mark.parametrize(
    ("distance", "expected"),
    [(5, 1.0), (6, 1.0), (8, 0.75), (10, 0.5), (12, 0.25), (14, 0.0), (15, 0.0)],
)
def test_a_feather_makes_the_edge_fall_linearly_across_its_width_centred_on_the_radius(
    distance: int, expected: float
) -> None:
    """A disc of radius 10 with a feather of 8 px: full up to 6 px from its centre, nothing from 14 px."""
    layer = one((30.0, 20.0), (30.0, 20.0), radius=10.0, feather=8.0)
    assert layer[20, 30 + distance] == pytest.approx(expected)
    assert layer[20, 30 - distance] == pytest.approx(expected)


@pytest.mark.parametrize("feather", [0.0, 0.2, 0.99])
def test_a_feather_under_one_pixel_is_drawn_as_one_pixel(feather: float) -> None:
    assert np.array_equal(
        one((30.0, 20.0), (30.0, 20.0), radius=3.0, feather=feather), one((30.0, 20.0), (30.0, 20.0), radius=3.0)
    )


@pytest.mark.parametrize(
    ("start", "end"),
    [
        ((10.0, 5.0), (40.0, 35.0)),
        ((40.0, 35.0), (10.0, 5.0)),
        ((40.0, 5.0), (10.0, 35.0)),
        ((10.0, 35.0), (40.0, 5.0)),
    ],
    ids=["down right", "up left", "down left", "up right"],
)
def test_a_segment_looks_the_same_whichever_way_round_its_ends_are_given_and_whichever_way_it_runs(
    start: tuple[float, float], end: tuple[float, float]
) -> None:
    """Every one of these diagonals passes through the pixel (25, 20): it is on the line. The pixel next to it is
    1 / sqrt 2 = 0.71 px from the line, so covered by 1 + 0.5 - 0.71 = 0.79; the one 3 px away is 2.1 px from it."""
    layer = one(start, end, radius=1.0)
    assert layer[20, 25] == 1.0
    assert layer[20, 26] == pytest.approx(0.79, abs=0.005)
    assert layer[20, 28] == 0.0


def test_a_weight_scales_the_brightness_and_overlapping_segments_show_the_brighter_one() -> None:
    starts, ends = np.array([[10.0, 20.0], [30.0, 10.0]]), np.array([[50.0, 20.0], [30.0, 30.0]])
    layer = draw_segments(SHAPE, starts, ends, np.array([2.0, 2.0]), np.array([0.4, 0.7]), 1.0)
    assert layer[20, 15] == pytest.approx(0.4)  # only the first
    assert layer[12, 30] == pytest.approx(0.7)  # only the second
    assert layer[20, 30] == pytest.approx(0.7)  # where they cross: the brighter, not 1.1 and not 0.4


def test_every_segment_has_its_own_radius_weight_and_feather_when_given_arrays() -> None:
    starts = ends = np.array([[10.0, 10.0], [40.0, 30.0]])
    layer = draw_segments(SHAPE, starts, ends, np.array([2.0, 5.0]), np.array([1.0, 0.5]), np.array([1.0, 4.0]))
    assert layer[10, 12] == pytest.approx(0.5)  # a radius of 2, 2 px from its centre
    assert layer[30, 45] == pytest.approx(0.25)  # a radius of 5 and weight 0.5, 5 px from its centre: half of half
    assert layer[30, 46] == pytest.approx(0.125)  # feather 4: 6 px is 1 px into the ramp's last half


def test_parts_beyond_the_picture_edge_are_cut_off() -> None:
    layer = one((50.0, 20.0), (90.0, 20.0), radius=2.0)  # starts inside, runs out of the right edge (x = 59)
    assert layer.shape == SHAPE
    np.testing.assert_array_equal(layer[20, 49:60], 1.0)  # from 1 px behind the start to the last column
    assert layer[20, 48] == 0.5  # the radius behind the start


def test_no_segments_leave_a_black_layer_of_the_shape_in_float32() -> None:
    layer = draw_segments(SHAPE, np.empty((0, 2)), np.empty((0, 2)), 2.0, 1.0, 1.0)
    assert layer.shape == SHAPE
    assert layer.dtype == np.float32
    assert not layer.any()


def test_a_slanted_segment_is_drawn_by_the_perpendicular_distance_from_its_line() -> None:
    """The segment from (10, 5) to (40, 35) runs at 45 degrees. The pixel (x=30, y=20) is (20, 15) from its start, so
    the cross product of that with the direction (1, 1) / sqrt 2 gives its distance from the line: 5 / sqrt 2. With a
    radius of exactly that distance it is half covered."""
    layer = one((10.0, 5.0), (40.0, 35.0), radius=5 / np.sqrt(2))
    assert layer[20, 30] == pytest.approx(0.5, abs=1e-5)
