"""Behaviour of `bullterrier_image.ops.relief`: the slopes of a height field."""

import numpy as np
import pytest

from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.relief import slopes

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
