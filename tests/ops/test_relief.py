"""Behaviour of `bullterrier_image.ops.relief`: the slopes of a height field and the light a surface catches."""

import numpy as np
import pytest

from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.relief import lambert, slopes

HEIGHT = np.array([[0.0, 1.0, 4.0, 9.0], [2.0, 2.0, 2.0, 2.0], [5.0, 3.0, 1.0, 0.0]])


class TestSlopes:
    def test_inside_a_slope_is_half_the_difference_of_the_neighbours_times_the_scale(self) -> None:
        rise_y, rise_x = slopes(HEIGHT, scale=2.0, edge="reflect")
        assert rise_x[0, 1] == 2.0 * (4.0 - 0.0) / 2  # along the first row, from 0 to 4
        assert rise_x[2, 2] == 2.0 * (0.0 - 3.0) / 2  # down along the last row, from 3 to 0
        assert rise_y[1, 0] == 2.0 * (5.0 - 0.0) / 2  # down the first column, from 0 to 5
        assert rise_y[1, 3] == 2.0 * (0.0 - 9.0) / 2

    @pytest.mark.parametrize(
        ("edge", "first_row_rise_y", "first_column_rise_x", "last_column_rise_x"),
        [
            pytest.param(
                "reflect", (2 - 0) / 2, (1 - 0) / 2, (9 - 4) / 2, id="reflect: the edge pixel is its own neighbour"
            ),
            pytest.param(
                "nearest", (2 - 0) / 2, (1 - 0) / 2, (9 - 4) / 2, id="nearest: the edge pixel is its own neighbour"
            ),
            pytest.param(
                "mirror", (2 - 2) / 2, (1 - 1) / 2, (4 - 4) / 2, id="mirror: the pixel beside the edge, twice"
            ),
            pytest.param("wrap", (2 - 5) / 2, (1 - 9) / 2, (0 - 4) / 2, id="wrap: the opposite edge's pixel"),
        ],
    )
    def test_edge_decides_the_slope_of_the_border_row_and_column_and_nothing_inside(
        self, edge: Edge, first_row_rise_y: float, first_column_rise_x: float, last_column_rise_x: float
    ) -> None:
        rise_y, rise_x = slopes(HEIGHT, scale=1.0, edge=edge)
        assert rise_y[0, 0] == first_row_rise_y
        assert rise_x[0, 0] == first_column_rise_x
        assert rise_x[0, 3] == last_column_rise_x
        reference_y, reference_x = slopes(HEIGHT, scale=1.0, edge="reflect")
        np.testing.assert_array_equal(rise_y[1:-1], reference_y[1:-1])
        np.testing.assert_array_equal(rise_x[:, 1:-1], reference_x[:, 1:-1])

    def test_a_constant_height_has_no_slope_and_the_input_is_left_alone(self) -> None:
        flat = np.full((5, 4), 0.7, np.float32)
        before = flat.copy()
        for edge in ("reflect", "nearest", "mirror", "wrap"):
            rise_y, rise_x = slopes(flat, scale=9.0, edge=edge)
            assert not rise_y.any()
            assert not rise_x.any()
        np.testing.assert_array_equal(flat, before)


class TestLambert:
    @pytest.mark.parametrize("elevation", [0, 30, 90])
    def test_a_flat_surface_catches_the_sine_of_the_elevation_whatever_the_angle(self, elevation: float) -> None:
        zero = np.zeros((2, 2))
        for angle in (0, 77, 270):
            np.testing.assert_allclose(
                lambert(zero, zero, angle=angle, elevation=elevation), np.sin(np.radians(elevation)), atol=1e-12
            )

    @pytest.mark.parametrize(
        ("angle", "rise_y", "rise_x", "elevation", "expected"),
        [
            pytest.param(270, 0, 0.75, 0, 0.6, id="light from the left, surface rising right at 3/4: leans 3/5 to it"),
            pytest.param(90, 0, 0.75, 0, -0.6, id="the same surface lit from the right: leans 3/5 away"),
            pytest.param(0, 0.75, 0, 0, 0.6, id="light from above, surface rising downwards: leans to it"),
            pytest.param(0, -0.75, 0, 0, -0.6, id="light from above, surface rising upwards: leans away"),
            pytest.param(180, -0.75, 0, 0, 0.6, id="light from below, surface rising upwards: leans to it"),
            pytest.param(270, 0, 0.75, 90, 0.8, id="light straight above: the normal's own height, 1 / (5/4)"),
            pytest.param(270, 0, 0.75, 53.130102354, 1.0, id="light along the normal (3/5 over, 4/5 up) catches all"),
            pytest.param(0, 0.3, 0.4, 90, 1 / np.sqrt(1.25), id="a slope of 0.5 whichever way: 1 / sqrt(1.25)"),
        ],
    )
    def test_the_catch_is_the_dot_product_of_the_unit_normal_and_the_light_worked_by_hand(
        self, angle: float, rise_y: float, rise_x: float, elevation: float, expected: float
    ) -> None:
        caught = lambert(np.array([[rise_y]]), np.array([[rise_x]]), angle=angle, elevation=elevation)
        assert caught[0, 0] == pytest.approx(expected, abs=1e-9)
