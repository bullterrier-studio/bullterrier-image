"""Behaviour of the `polar` effect: the picture wrapped round a centre as a planet (to) or unwrapped from one (from)."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "polar"


def position_picture(height: int, width: int) -> np.ndarray:
    """A picture whose red is its row / (height - 1) and green its column / (width - 1): each pixel says where it is, so
    a warped pixel shows where it was read from."""
    rows, cols = np.mgrid[0:height, 0:width].astype(np.float64)
    return np.dstack([rows / (height - 1), cols / (width - 1), np.zeros_like(rows)]).astype(np.float32)


@pytest.mark.parametrize(
    ("pixel", "expected_red", "expected_green"),
    [
        ((50, 75), 0.50, 0.2525),
        ((10, 50), 0.20, 0.0),
        ((80, 50), 0.40, 0.505),
        ((50, 20), 0.40, 0.7575),
    ],
    ids=["right of the centre", "above it", "below it", "left of it"],
)
def test_to_lays_the_columns_round_the_centre_clockwise_from_the_top_and_the_rows_out_from_the_bottom(
    pixel: tuple[int, int], expected_red: float, expected_green: float
) -> None:
    """On 101 x 101 px the planet's centre is pixel (50, 50) and its rim, the top row, is 50 px out; the bottom row is
    at the centre. A pixel d px out reads the row 100 (1 - d / 50) and, at the fraction of a whole turn clockwise from
    straight up, that fraction of the 101 columns. Right of the centre by 25 px: row 50, a quarter turn, column 25.25
    (red 0.5, green 25.25 / 100); above by 40: row 20, no turn, column 0; below by 30: row 40, half a turn, column
    50.5; left by 30: row 40, three quarters of a turn, column 75.75."""
    out = registry.apply(NAME, position_picture(101, 101), mode="to", interpolation="bilinear")
    assert out[pixel][0] == pytest.approx(expected_red, abs=1e-5)
    assert out[pixel][1] == pytest.approx(expected_green, abs=1e-5)


def test_to_goes_on_from_the_last_column_to_the_first_across_the_seam_above_the_centre() -> None:
    """The pixel just left of straight up (offset -1 px across, 40 up) is 0.025 rad short of a whole turn, column
    101 - 101 x 0.024995 / (2 pi) = 100.598 of 101: between the last column (green 1) and the first (0), the
    first being the column after the last: green 1 - 0.598 = 0.4018."""
    out = registry.apply(NAME, position_picture(101, 101), mode="to", interpolation="bilinear")
    assert out[10, 49, 1] == pytest.approx(0.4018, abs=1e-4)


@pytest.mark.parametrize(
    ("pixel", "expected_red", "expected_green"),
    [
        ((0, 25), 0.5, 1.0),
        ((33, 25), 0.5, 82.5 / 99),
        ((33, 0), 16.5 / 99, 0.5),
        ((33, 50), 82.5 / 99, 0.5),
        ((99, 25), 0.5, 0.5),
    ],
    ids=[
        "the top row is the rim, a quarter turn: right",
        "two thirds out, a quarter turn",
        "no turn: up",
        "half a turn: down",
        "the bottom row is the centre",
    ],
)
def test_from_reads_the_picture_as_a_planet_at_the_polar_coordinates_of_each_pixel(
    pixel: tuple[int, int], expected_red: float, expected_green: float
) -> None:
    """On 100 x 100 px the planet's centre is (49.5, 49.5) and its rim is 49.5 px out. The pixel of row r and column c
    reads the planet at the distance 49.5 (1 - r / 99) from the centre, at c / 100 of a turn clockwise from up: row 0,
    column 25: 49.5 right of the centre, (49.5, 99), red 0.5, green 1; row 33: 33 px out, so a quarter turn reads
    (49.5, 82.5); no turn (column 0) (16.5, 49.5); half a turn (column 50) (82.5, 49.5); row 99 is the centre
    itself, (49.5, 49.5), at any angle."""
    out = registry.apply(NAME, position_picture(100, 100), mode="from", interpolation="bilinear")
    assert out[pixel][0] == pytest.approx(expected_red, abs=1e-5)
    assert out[pixel][1] == pytest.approx(expected_green, abs=1e-5)


@pytest.mark.parametrize("interpolation", ["bilinear", "bicubic"])
def test_to_then_from_gives_back_a_smooth_picture_away_from_the_bottom(interpolation: str) -> None:
    """Unwrapping the planet is the inverse of wrapping it: away from the bottom rows, which lie so near the
    centre that the planet has too few pixels to hold their columns, a picture that is smooth and periodic across
    its columns comes back to within the interpolation's error."""
    rows, cols = np.mgrid[0:129, 0:129].astype(np.float64)
    turn = 2 * np.pi * cols / 129
    smooth = 0.5 + 0.2 * np.sin(turn) * (rows / 128) + 0.2 * np.cos(2 * turn) * (1 - rows / 128)
    picture = np.repeat(smooth.astype(np.float32)[..., None], 3, axis=2)
    planet = registry.apply(NAME, picture, mode="to", interpolation=interpolation)
    back = registry.apply(NAME, planet, mode="from", interpolation=interpolation)
    kept = slice(0, 77)  # rows 0 to 76 of 129: at least 40% of the way out
    np.testing.assert_allclose(back[kept], picture[kept], atol=0.02)
    assert np.abs(planet - picture).max() > 0.1  # and the planet is not the picture


@pytest.mark.parametrize("mode", ["to", "from"])
def test_the_centre_of_the_planet_is_the_bottom_row_and_x_and_y_move_it(mode: str) -> None:
    """With 'to' the pixel at the centre (x, y) shows the bottom row, which is flat here; with 'from' the bottom row
    shows the one pixel at the centre, whatever the angle."""
    rng = np.random.default_rng(9)
    noise = rng.random((61, 81, 3), dtype=np.float32)
    centre = (45, 20)  # y = 0.75 of the 61 rows and x = 0.25 of the 81 columns
    if mode == "to":
        noise[-1] = 0.7
        out = registry.apply(NAME, noise, mode="to", x=0.25, y=0.75, interpolation="bilinear")
        np.testing.assert_allclose(out[centre], 0.7, atol=1e-6)
    else:
        out = registry.apply(NAME, noise, mode="from", x=0.25, y=0.75, interpolation="bilinear")
        np.testing.assert_allclose(out[-1], np.broadcast_to(noise[centre], out[-1].shape), atol=1e-6)


@pytest.mark.parametrize(
    ("edge", "wanted"),
    [("nearest", True), ("reflect", False), ("mirror", False), ("wrap", False)],
    ids=["nearest keeps the sky flat", "reflect", "mirror", "wrap"],
)
def test_beyond_the_rim_of_the_planet_the_edge_rule_says_what_the_sky_is(edge: str, wanted: bool) -> None:
    """The corners lie more than 50 px from the centre, beyond the top row: 'nearest' repeats the top row, here a
    flat 0.9, so they show 0.9; the other rules read the noise below it."""
    noise = np.random.default_rng(10).random((101, 101, 3), dtype=np.float32)
    noise[0] = 0.9
    out = registry.apply(NAME, noise, mode="to", edge=edge, interpolation="nearest")
    corners = out[[0, 0, 100, 100], [0, 100, 0, 100]]
    assert np.allclose(corners, 0.9, atol=1e-6) == wanted


def test_from_leaves_what_an_off_centre_planet_reads_beyond_the_picture_to_the_edge_rule_not_the_columns() -> None:
    """The planet's centre on the right edge of a picture of 65 rows and 64 columns: the pixel of the top row (the
    rim) a quarter turn round (column 16 of 64) reads 31.5 px right of the centre, beyond the last column; 'nearest'
    repeats it, so it shows the last column at the centre's row, 32, and does not wrap round to the first."""
    noise = np.random.default_rng(11).random((65, 64, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, mode="from", x=1.0, y=0.5, edge="nearest", interpolation="nearest")
    np.testing.assert_array_equal(out[0, 16], noise[32, 63])


@pytest.mark.parametrize("mode", ["to", "from"])
@pytest.mark.parametrize("shape", [(1, 1), (1, 7), (7, 1)], ids=["a pixel", "a row", "a column"])
def test_a_picture_with_no_planet_to_fit_in_it_is_returned_as_it_is(mode: str, shape: tuple[int, int]) -> None:
    """The planet is half the shorter side wide, which a single row or column does not have."""
    noise = np.random.default_rng(12).random((*shape, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, mode=mode)
    np.testing.assert_array_equal(out, noise)
    assert not np.shares_memory(out, noise)
