import dataclasses

import numpy as np
import pytest

from photo_fx.ops.distance import EdgeField, edge_distance

HEIGHT, WIDTH = 40, 60
TOP, BOTTOM, LEFT, RIGHT = 10, 29, 15, 44  # inclusive bounds of the rectangle in the middle of the array


def rectangle() -> np.ndarray:
    shape = np.zeros((HEIGHT, WIDTH), dtype=bool)
    shape[TOP : BOTTOM + 1, LEFT : RIGHT + 1] = True
    return shape


def nearest_side_distance(sampling: tuple[float, float]) -> np.ndarray:
    """Independent oracle: inside a rectangle the distance to the outside is the distance to the nearest side."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    to_sides = np.stack(
        [
            (rows - (TOP - 1)) * sampling[0],
            ((BOTTOM + 1) - rows) * sampling[0],
            (cols - (LEFT - 1)) * sampling[1],
            ((RIGHT + 1) - cols) * sampling[1],
        ]
    )
    return np.where(rectangle(), to_sides.min(axis=0), 0.0)


@pytest.mark.parametrize("sampling", [(1.0, 1.0), (2.0, 1.0), (1.0, 3.0), (7.0, 5.0)])
def test_distance_in_a_rectangle_is_the_distance_to_the_nearest_side_times_sampling(sampling):
    field = edge_distance(rectangle(), sampling)
    np.testing.assert_allclose(field.distance, nearest_side_distance(sampling), atol=1e-9)


def test_distance_at_the_centre_and_outside_the_shape():
    field = edge_distance(rectangle())
    assert abs(field.distance[19, 29] - 10.0) <= 1.0  # 10 px from the top and bottom sides, 15 from left/right
    assert np.all(field.distance[~rectangle()] == 0.0)
    assert np.all(field.distance[rectangle()] > 0.0)


def test_distance_rises_towards_the_centre():
    distance = edge_distance(rectangle()).distance
    down_from_top = distance[TOP : TOP + 10, 30]
    assert np.all(np.diff(down_from_top) > 0)
    right_from_left = distance[19, LEFT : LEFT + 15]
    assert np.all(np.diff(right_from_left) >= 0)
    assert right_from_left[-1] > right_from_left[0]


def test_anisotropic_sampling_doubles_vertical_distances_only():
    plain = edge_distance(rectangle(), (1.0, 1.0)).distance
    tall = edge_distance(rectangle(), (2.0, 1.0)).distance
    # Straight below the top side and straight right of the left side, away from the far sides.
    assert tall[TOP + 3, 30] == 2 * plain[TOP + 3, 30] == 8.0
    assert tall[19, LEFT + 3] == plain[19, LEFT + 3] == 4.0


@pytest.mark.parametrize("sampling", [(1.0, 1.0), (2.0, 1.0), (7.0, 5.0)])
@pytest.mark.parametrize(
    ("row", "col", "expected_x", "expected_y"),
    [
        (19, LEFT + 2, -1.0, 0.0),  # left side: points left
        (19, RIGHT - 2, 1.0, 0.0),  # right side: points right
        (TOP + 2, 30, 0.0, -1.0),  # top side: points up (y is down)
        (BOTTOM - 2, 30, 0.0, 1.0),  # bottom side: points down
    ],
)
def test_normals_point_out_of_the_shape_on_each_side(row, col, expected_x, expected_y, sampling):
    # Stretched elements (the disperse/evaporate case: characters are taller than wide) must not turn the normals.
    field = edge_distance(rectangle(), sampling)
    assert field.normal_x[row, col] == pytest.approx(expected_x, abs=0.1)
    assert field.normal_y[row, col] == pytest.approx(expected_y, abs=0.1)


def test_normals_are_unit_length_everywhere():
    field = edge_distance(rectangle(), (2.0, 3.0))
    np.testing.assert_allclose(np.hypot(field.normal_x, field.normal_y), 1.0, atol=1e-9)


def test_normal_points_up_where_the_distance_is_flat():
    field = edge_distance(np.zeros((6, 9), dtype=bool))
    assert np.all(field.normal_x == 0.0)
    assert np.all(field.normal_y == -1.0)


def test_left_right_and_top_borders_of_the_array_are_edges():
    band = np.zeros((HEIGHT, WIDTH), dtype=bool)
    band[:20, :] = True  # touches left, right and top
    distance = edge_distance(band).distance
    assert distance[0, 30] == 1.0  # next to the top border
    assert distance[10, 0] == 1.0  # next to the left border
    assert distance[10, WIDTH - 1] == 1.0  # next to the right border
    assert distance[10, 30] == 10.0  # the band's lower side (row 20 is outside) is 10 away, the top border 11


def test_bottom_border_is_an_edge_only_when_open_bottom_is_false():
    shape = np.zeros((HEIGHT, WIDTH), dtype=bool)
    shape[20:, 10:50] = True  # touches the bottom
    open_bottom = edge_distance(shape).distance
    closed_bottom = edge_distance(shape, open_bottom=False).distance
    assert open_bottom[HEIGHT - 1, 30] == 20.0  # only the top side (row 19) is near enough: the shape carries on
    assert closed_bottom[HEIGHT - 1, 30] == 1.0
    assert open_bottom[30, 30] == 11.0
    assert closed_bottom[30, 30] == 10.0  # now the bottom border (row 40) is nearer than the top side


@pytest.mark.parametrize("open_bottom", [True, False])
def test_a_shape_that_ends_above_the_last_row_keeps_its_bottom_edge(open_bottom):
    shape = np.zeros((HEIGHT, WIDTH), dtype=bool)
    shape[10:38, 10:50] = True  # rows 38 and 39 are empty
    distance = edge_distance(shape, open_bottom=open_bottom).distance
    assert distance[37, 30] == 1.0  # its own bottom side, whatever the array's bottom border does
    assert distance[24, 30] == 14.0  # 14 from the top side (row 9) and from the bottom side (row 38)
    assert distance[38, 30] == distance[39, 30] == 0.0


def test_open_bottom_depends_on_the_last_row_even_when_it_differs_from_the_row_above():
    shape = np.zeros((HEIGHT, WIDTH), dtype=bool)
    shape[20:, 10:50] = True
    shape[38, 20:30] = False  # a slit in the row above the last one
    open_bottom = edge_distance(shape).distance
    closed_bottom = edge_distance(shape, open_bottom=False).distance
    assert open_bottom[HEIGHT - 1, 25] == 1.0  # right under the slit, in both cases
    assert closed_bottom[HEIGHT - 1, 25] == 1.0
    assert open_bottom[HEIGHT - 1, 35] == pytest.approx(np.hypot(1, 6))  # the slit's corner (38, 29) is nearest
    assert closed_bottom[HEIGHT - 1, 35] == 1.0  # the bottom border is nearer


def lone_element(size: int = 15) -> np.ndarray:
    shape = np.zeros((size, size), dtype=bool)
    shape[size // 2, size // 2] = True
    return shape


# offsets (rows down, columns right) from a lone element, within the 1-element smoothing's reach
NEIGHBOURS = [(dy, dx) for dy in (-2, -1, 0, 1, 2) for dx in (-2, -1, 0, 1, 2) if (dy, dx) != (0, 0)]


@pytest.mark.parametrize("sampling", [(1.0, 1.0), (2.0, 1.0), (1.0, 3.0)])
def test_beside_a_lone_element_the_normal_points_away_from_it(sampling):
    field = edge_distance(lone_element(), sampling)
    for dy, dx in NEIGHBOURS:
        nx, ny = field.normal_x[7 + dy, 7 + dx], field.normal_y[7 + dy, 7 + dx]
        assert (np.sign(nx), np.sign(ny)) == (np.sign(dx), np.sign(dy)), (dy, dx)  # out: right/left, down/up
        if dy == 0:
            assert (nx, ny) == pytest.approx((np.sign(dx), 0.0), abs=1e-9)  # straight along the row
        if dx == 0:
            assert (nx, ny) == pytest.approx((0.0, np.sign(dy)), abs=1e-9)  # straight along the column


@pytest.mark.parametrize("sampling", [(2.0, 1.0), (1.0, 3.0), (7.0, 5.0)])
def test_stretched_elements_tilt_the_normal_as_a_gradient_in_px_does(sampling):
    # The normal is the negative gradient per px. Smoothing is over elements whatever their size, so stretching the
    # elements divides the x slope by the column width and the y slope by the row height: the ratio of the normal's
    # components changes by row height / column width. Taller elements tilt it towards x, wider ones towards y.
    row_height, col_width = sampling
    square, stretched = edge_distance(lone_element()), edge_distance(lone_element(), sampling)
    for dy, dx in [(1, 2), (-2, 1), (1, -1), (-1, -2)]:
        y, x = 7 + dy, 7 + dx
        expected = square.normal_x[y, x] / square.normal_y[y, x] * row_height / col_width
        assert stretched.normal_x[y, x] / stretched.normal_y[y, x] == pytest.approx(expected, rel=1e-6), (dy, dx)


def test_far_beyond_a_shape_the_distance_is_flat_so_the_normal_points_up():
    shape = np.zeros((20, 60), dtype=bool)
    shape[10, 2] = True
    field = edge_distance(shape)
    # 1-element smoothing does not reach 30 elements away: no direction there, so up
    assert np.all(field.normal_x[:, 32:] == 0.0) and np.all(field.normal_y[:, 32:] == -1.0)


def test_a_shape_that_is_all_true_has_edges_at_its_borders_but_not_below():
    shape = np.ones((8, 8), dtype=bool)
    open_bottom = edge_distance(shape).distance
    assert open_bottom[0, 3] == 1.0
    assert open_bottom[7, 3] == 4.0  # from the top border (row -1) it is 8 away; from the left/right border 4
    assert edge_distance(shape, open_bottom=False).distance[7, 3] == 1.0


@pytest.mark.parametrize(
    "shape", [np.zeros((5, 5), bool), np.ones((5, 5), bool), np.ones((1, 6), bool), np.ones((6, 1), bool)]
)
def test_degenerate_shapes_give_finite_results_with_unit_normals(shape):
    field = edge_distance(shape)
    for array in (field.distance, field.normal_x, field.normal_y):
        assert array.shape == shape.shape
        assert np.isfinite(array).all()
    np.testing.assert_allclose(np.hypot(field.normal_x, field.normal_y), 1.0, atol=1e-9)


def test_edge_distance_does_not_modify_its_input_and_returns_an_immutable_record():
    shape = rectangle()
    before = shape.copy()
    field = edge_distance(shape)
    np.testing.assert_array_equal(shape, before)
    assert isinstance(field, EdgeField)
    with pytest.raises(dataclasses.FrozenInstanceError):
        field.distance = field.normal_x  # type: ignore[misc]


@pytest.mark.parametrize(
    "shape",
    [
        np.ones((4, 4), dtype=np.uint8),  # not bool
        np.ones((4, 4), dtype=float),
        np.ones(4, dtype=bool),  # 1-D
        np.ones((2, 4, 4), dtype=bool),  # 3-D
        np.ones((0, 4), dtype=bool),  # empty
        [[True, False], [False, True]],  # not an array
    ],
)
def test_shape_must_be_a_non_empty_2d_bool_array(shape):
    with pytest.raises(ValueError, match="shape"):
        edge_distance(shape)


@pytest.mark.parametrize(
    "sampling", [(0.0, 1.0), (1.0, 0.0), (-1.0, 1.0), (1.0, -2.0), (1.0,), (1.0, 1.0, 1.0), (float("nan"), 1.0)]
)
def test_sampling_must_be_two_positive_numbers(sampling):
    with pytest.raises(ValueError, match="sampling"):
        edge_distance(rectangle(), sampling)
