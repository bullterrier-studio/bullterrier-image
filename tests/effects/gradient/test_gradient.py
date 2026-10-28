"""Behaviour of the `gradient` effect: a generated colour gradient (linear, radial or conic) in place of the picture."""

import numpy as np
import pytest

from bullterrier_image import Recipe, RecipeError, Step
from bullterrier_image import render as render_recipe
from bullterrier_image.core import registry

NAME = "gradient"
RED, GREEN, BLUE = (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)
HEX = {RED: "#ff0000", GREEN: "#00ff00", BLUE: "#0000ff"}


def render(shape: tuple[int, int] = (101, 101), colors: tuple = (RED, BLUE), **settings: object) -> np.ndarray:
    """The gradient over a picture of `shape` (h, w); `colors` are (r, g, b) triples given as hex strings."""
    picture = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, picture, colors=[HEX[c] for c in colors], **settings)


def mix(first: tuple, second: tuple, t: float) -> np.ndarray:
    """The colour a fraction t of the way from `first` to `second`, written out by hand."""
    return (1 - t) * np.array(first) + t * np.array(second)


def test_a_linear_gradient_runs_from_the_first_colour_to_the_last_across_the_picture() -> None:
    """At angle 0 the position along the picture is the pixel centre's fraction of the width, (x + 0.5) / 100."""
    out = render((10, 100), kind="linear", angle=0)
    for x in (0, 17, 50, 99):
        np.testing.assert_allclose(out[:, x], np.tile(mix(RED, BLUE, (x + 0.5) / 100), (10, 1)), atol=1e-5)


@pytest.mark.parametrize(
    ("angle", "first_at", "last_at"),
    [
        pytest.param(0, (50, 0), (50, 100), id="0 degrees runs left to right"),
        pytest.param(90, (0, 50), (100, 50), id="90 degrees runs top to bottom"),
        pytest.param(180, (50, 100), (50, 0), id="180 degrees runs right to left"),
        pytest.param(270, (100, 50), (0, 50), id="270 degrees runs bottom to top"),
        pytest.param(45, (0, 0), (100, 100), id="45 degrees runs from the top-left corner to the bottom-right"),
    ],
)
def test_the_angle_sets_the_direction_the_linear_gradient_runs(angle: float, first_at: tuple, last_at: tuple) -> None:
    out = render(kind="linear", angle=angle)
    assert np.linalg.norm(out[first_at] - RED) < 0.03
    assert np.linalg.norm(out[last_at] - BLUE) < 0.03


def test_the_middle_pixel_of_a_three_colour_gradient_is_its_middle_colour() -> None:
    """The colours sit at 0, 1/2 and 1; on an odd width the middle column has position exactly 1/2."""
    out = render((5, 101), colors=(RED, GREEN, BLUE), kind="linear", angle=0)
    np.testing.assert_allclose(out[:, 50], np.tile(GREEN, (5, 1)), atol=1e-6)


def test_the_midpoint_between_two_colours_is_their_mean() -> None:
    out = render((4, 100), kind="linear", angle=0)  # even width: no column at 1/2, the two beside it average to it
    np.testing.assert_allclose((out[:, 49] + out[:, 50]) / 2, np.tile(mix(RED, BLUE, 0.5), (4, 1)), atol=1e-5)


def test_a_diagonal_band_is_as_long_as_the_picture_is_along_it() -> None:
    """At 45 degrees on a 101 px square the band is 101 x (cos + sin) = 101 x sqrt 2 long; the pixel 25 px right and
    25 px down of the centre has moved 25 x sqrt 2 along it, so its position is 1/2 + 25 / 101."""
    out = render(kind="linear", angle=45)
    np.testing.assert_allclose(out[75, 75], mix(RED, BLUE, 0.5 + 25 / 101), atol=1e-5)


def test_a_linear_gradient_ignores_the_centre_settings() -> None:
    np.testing.assert_array_equal(render(kind="linear", x=0.1, y=0.9), render(kind="linear"))


def test_a_linear_gradient_is_constant_along_lines_perpendicular_to_its_direction() -> None:
    """At 45 degrees the direction is (1, 1) in (x, y), so the pixel one down and one left has the same position."""
    out = render((60, 60), kind="linear", angle=45)
    np.testing.assert_allclose(out[1:, :-1], out[:-1, 1:], atol=1e-5)


def test_a_radial_gradient_has_the_first_colour_at_its_centre_and_is_symmetric_about_it() -> None:
    out = render(kind="radial", radius=0.5)
    np.testing.assert_allclose(out[50, 50], RED, atol=1e-6)
    np.testing.assert_array_equal(out, out[::-1])
    np.testing.assert_array_equal(out, out[:, ::-1])
    np.testing.assert_allclose(out, out.transpose(1, 0, 2), atol=1e-6)


def test_a_radial_gradient_mixes_by_distance_over_the_radius_and_stays_on_the_last_colour_beyond_it() -> None:
    """Radius 0.5 of the shorter side (101 px) is 50.5 px; the pixel 30 px right of the centre is at 30 / 50.5."""
    out = render(kind="radial", radius=0.5)
    np.testing.assert_allclose(out[50, 80], mix(RED, BLUE, 30 / 50.5), atol=1e-5)
    np.testing.assert_allclose(out[0, 0], BLUE, atol=1e-6)  # the corner, 70 px away, is past the radius


def test_the_radius_is_a_fraction_of_the_shorter_side() -> None:
    """On a 61 x 201 picture a radius of 0.5 is 30.5 px, however wide the picture is: 20 px right of the centre."""
    out = render((61, 201), kind="radial", radius=0.5)
    np.testing.assert_allclose(out[30, 120], mix(RED, BLUE, 20 / 30.5), atol=1e-5)  # centre pixel (30, 100)


def test_the_centre_moves_the_radial_gradient() -> None:
    out = render((100, 100), kind="radial", x=0.25, y=0.75, radius=0.5)
    nearest_first = np.unravel_index(np.argmin(np.linalg.norm(out - RED, axis=-1)), (100, 100))
    assert abs(nearest_first[0] - 75) <= 1  # row: y = 0.75 of the height
    assert abs(nearest_first[1] - 25) <= 1  # column: x = 0.25 of the width


@pytest.mark.parametrize(
    ("angle", "pixel", "turn"),
    [
        pytest.param(0, (50, 100), 0.0, id="start at 0 degrees: right of the centre is the start"),
        pytest.param(0, (100, 50), 0.25, id="clockwise on screen: below the centre is a quarter turn"),
        pytest.param(0, (50, 0), 0.5, id="left of the centre is half a turn"),
        pytest.param(0, (0, 50), 0.75, id="above the centre is three quarters of a turn"),
        pytest.param(90, (100, 50), 0.0, id="start at 90 degrees: below the centre is the start"),
        pytest.param(90, (50, 0), 0.25, id="start at 90 degrees: a quarter turn later is left"),
    ],
)
def test_a_conic_gradient_sweeps_clockwise_from_the_start_angle(angle: float, pixel: tuple, turn: float) -> None:
    out = render(kind="conic", angle=angle)
    np.testing.assert_allclose(out[pixel], mix(RED, BLUE, turn), atol=1e-5)


def test_repeat_fits_the_colours_in_again_along_the_gradient() -> None:
    out = render((3, 100), kind="linear", angle=0, repeat=2)
    np.testing.assert_allclose(out[:, :50], out[:, 50:], atol=1e-5)
    np.testing.assert_allclose(out[:, 25], np.tile(mix(RED, BLUE, 25.5 / 50), (3, 1)), atol=1e-5)


def test_reflect_runs_every_second_repeat_backwards() -> None:
    out = render((3, 100), kind="linear", angle=0, repeat=2, reflect=True)
    np.testing.assert_allclose(out, out[:, ::-1], atol=1e-5)
    np.testing.assert_allclose(
        out[:, 0], np.tile(mix(RED, BLUE, 0.01), (3, 1)), atol=1e-5
    )  # the first run goes forward
    np.testing.assert_allclose(out[:, 49], out[:, 50], atol=1e-5)  # the two runs meet on the last colour


def test_repeat_on_a_radial_gradient_ends_on_the_last_colour_beyond_the_radius() -> None:
    out = render(kind="radial", radius=0.5, repeat=3)
    np.testing.assert_allclose(out[0, 0], BLUE, atol=1e-6)


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param({"kind": "linear", "angle": 30}, id="linear"),
        pytest.param({"kind": "radial", "x": 0.2, "y": 0.8, "radius": 0.3}, id="radial"),
        pytest.param({"kind": "conic", "angle": 200, "repeat": 3, "reflect": True}, id="conic"),
    ],
)
def test_one_colour_is_a_flat_fill_whatever_the_layout(settings: dict) -> None:
    out = render((37, 53), colors=(GREEN,), **settings)
    np.testing.assert_array_equal(out, np.broadcast_to(np.array(GREEN, np.float32), (37, 53, 3)))


def test_a_gradient_needs_at_least_one_colour() -> None:
    with pytest.raises(RecipeError, match="colors"):
        registry.apply(NAME, np.zeros((4, 4, 3), np.float32), colors=[])


def test_the_fill_preset_is_a_flat_black_picture_that_ignores_the_input() -> None:
    """Preset `fill` keeps the old `fill` effect findable: black by default, any one colour with `colors`."""
    photo = np.random.default_rng(0).random((20, 30, 3), dtype=np.float32)
    black = render_recipe(photo, Recipe([Step("gradient", preset="fill")], name="t", size=None))
    np.testing.assert_array_equal(black, np.zeros((20, 30, 3), np.float32))
    green = render_recipe(
        photo, Recipe([Step("gradient", params={"colors": ["#00ff00"]}, preset="fill")], name="t", size=None)
    )
    np.testing.assert_array_equal(green, np.broadcast_to(np.array(GREEN, np.float32), (20, 30, 3)))


def test_the_fill_preset_masked_to_the_background_blacks_out_only_the_background() -> None:
    photo = np.random.default_rng(1).random((20, 30, 3), dtype=np.float32) * 0.5 + 0.25
    subject = np.zeros((20, 30), np.float32)
    subject[:, 15:] = 1
    recipe = Recipe([Step("gradient", preset="fill", mask="background")], name="t", size=None)
    out = render_recipe(photo, recipe, maps={"subject": subject})
    np.testing.assert_array_equal(out[:, 15:], photo[:, 15:])
    np.testing.assert_array_equal(out[:, :15], np.zeros((20, 15, 3), np.float32))


def test_the_fill_preset_and_the_summary_say_how_to_replace_the_background() -> None:
    assert 'mask = "background"' in registry.presets(NAME)["fill"].description
    assert 'mask = "background"' in registry.summary(NAME)
