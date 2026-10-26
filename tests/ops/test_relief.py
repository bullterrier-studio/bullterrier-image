"""Behaviour of `bullterrier_image.ops.relief`: the slopes of a height field and the light a surface catches."""

import numpy as np
import pytest

from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.relief import lambert, shade, slopes

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


def ramp(rise_per_px: float, size: int = 24) -> np.ndarray:
    """A height field that rises `rise_per_px` per px to the right (negative: to the left), level down the picture."""
    return np.tile(0.5 + rise_per_px * (np.arange(size) - size / 2), (size, 1)).astype(np.float32)


def hand_gain(slope_x: float, angle: float, elevation: float) -> float:
    """The gain a surface rising `slope_x` px per px to the right gets, from the geometry: the unit normal
    (-slope_x, 0, 1) / sqrt(1 + slope_x^2) dotted with the unit vector towards the light (x right, y down, z up), over
    what a flat surface (normal (0, 0, 1)) catches."""
    a, e = np.radians(angle), np.radians(elevation)
    light = np.array([np.cos(e) * np.sin(a), -np.cos(e) * np.cos(a), np.sin(e)])
    normal = np.array([-slope_x, 0.0, 1.0]) / np.hypot(slope_x, 1.0)
    return float(normal @ light / light[2])


def lit(
    surface: np.ndarray, *, height: float = 100.0, angle: float = 90.0, elevation: float = 45.0, blur: float = 0.0
) -> np.ndarray:
    """`shade` with the tests' usual light: from the right, 45 degrees up, on an unsmoothed relief 100 px tall."""
    return shade(surface, height=height, angle=angle, elevation=elevation, blur=blur, truncate=4.0, edge="reflect")


class TestShade:
    @pytest.mark.parametrize(
        ("surface", "settings"),
        [
            pytest.param(np.full((9, 7), 0.3, np.float32), {}, id="flat surface"),
            pytest.param(np.full((9, 7), 1.0, np.float32), {"angle": 200, "elevation": 1}, id="flat, grazing light"),
            pytest.param(np.full((9, 7), 0.0, np.float32), {"elevation": 90, "blur": 5}, id="flat, light above"),
            pytest.param(ramp(0.01), {"height": 0}, id="height 0 flattens a ramp"),
            pytest.param(
                np.random.default_rng(3).random((9, 7), dtype=np.float32), {"height": 0}, id="height 0, noise"
            ),
        ],
    )
    def test_a_flat_surface_gets_a_gain_of_1(self, surface: np.ndarray, settings: dict[str, float]) -> None:
        gain = lit(surface, **settings)
        np.testing.assert_allclose(gain, 1.0, atol=1e-6)

    @pytest.mark.parametrize(
        ("rise_per_px", "settings", "expected"),
        [
            pytest.param(-0.0075, {}, 1.4, id="rises away from a light at the right: faces it, (1 + 3/4) / (5/4)"),
            pytest.param(0.0075, {}, 0.2, id="rises towards a light at the right: faces away, (1 - 3/4) / (5/4)"),
            pytest.param(0.0075, {"angle": 270}, 1.4, id="the light moved to the left: the same slope faces it"),
            pytest.param(0.0075, {"angle": 0}, 0.8, id="light across the slope: only the tilt dims, 1 / (5/4)"),
            pytest.param(0.0075, {"elevation": 90}, 0.8, id="light straight above: only the tilt dims"),
            pytest.param(0.0075, {"angle": 270, "elevation": 20}, hand_gain(0.75, 270, 20), id="low light, left"),
            pytest.param(-0.004, {"angle": 60, "elevation": 70, "height": 50}, hand_gain(-0.2, 60, 70), id="any angle"),
        ],
    )
    def test_on_a_ramp_the_gain_is_lamberts_law_worked_out_by_hand(
        self, rise_per_px: float, settings: dict[str, float], expected: float
    ) -> None:
        """Inside the ramp, that is: the border columns see the edge rule."""
        gain = lit(ramp(rise_per_px), **settings)
        np.testing.assert_allclose(gain[:, 1:-1], expected, atol=1e-5)  # a float32 surface

    def test_blur_smooths_the_surface_so_a_spike_lights_less_steeply(self) -> None:
        spike = np.zeros((31, 31), np.float32)
        spike[15, 15] = 1.0
        spread = [float(np.ptp(lit(spike, height=5.0, blur=blur))) for blur in (0, 1, 3, 6)]
        assert spread == sorted(spread, reverse=True)
        assert len(set(spread)) == 4
        assert spread[-1] > 0

    def test_the_gain_is_a_float64_array_of_the_surfaces_shape_and_the_surface_is_left_alone(self) -> None:
        surface = np.random.default_rng(5).random((13, 21), dtype=np.float32)
        before = surface.copy()
        gain = lit(surface, blur=2.0)
        assert gain.dtype == np.float64
        assert gain.shape == (13, 21)
        np.testing.assert_array_equal(surface, before)
