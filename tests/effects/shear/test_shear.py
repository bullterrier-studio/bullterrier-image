"""Behaviour of the `shear` effect: the picture skewed about its centre, a row sliding sideways in proportion to how far
it is from the centre (`x`) and a column sliding up or down likewise (`y`)."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "shear"


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_shear_of_no_slope_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, x=0.0, y=0.0, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize(
    ("setting", "slope", "line_at"),
    [("x", 0.123, 30), ("x", -0.321, 70), ("x", 1.0, 50), ("y", 0.123, 30), ("y", -0.321, 70)],
    ids=[
        "x leans a vertical line right",
        "x negative leans left",
        "x of 1 is 45 degrees",
        "y tilts a horizontal line down",
        "y negative tilts up",
    ],
)
def test_a_line_across_the_shear_becomes_a_line_of_that_slope_through_the_centre_row_or_column(
    setting: str, slope: float, line_at: int
) -> None:
    """A white column `line_at` of a black 101 x 101 picture, centre row 50, sheared by `x`: a pixel d rows below the
    centre row moves x x d columns right, so the line is a line of slope x that crosses column `line_at` at the
    centre row, and 'nearest' puts each of its pixels on the closest column. `y` is the same with rows and columns
    swapped: a white row, moved down y px per column right of the centre column."""
    line = np.zeros((101, 101, 3), np.float32)
    line[:, line_at] = 1.0
    picture = line if setting == "x" else line.transpose(1, 0, 2).copy()
    out = registry.apply(NAME, picture, **{setting: slope}, interpolation="nearest")
    lit = out[..., 0] if setting == "x" else out[..., 0].T  # in both cases, one lit pixel per row, found in its columns
    expected = np.floor(line_at + slope * (np.arange(101) - 50) + 0.5)
    assert set(np.unique(out)) == {0.0, 1.0}  # 'nearest' keeps the picture's own values
    assert (lit.sum(axis=1) == 1).all()  # one white pixel in every row
    np.testing.assert_array_equal(lit.argmax(axis=1), expected)


@pytest.mark.parametrize(
    ("x", "y", "moved_to"),
    [(0.5, 0.25, (75, 70)), (-0.5, 0.25, (70, 50)), (0.0, 0.5, (75, 60)), (0.4, 0.0, (70, 68))],
    ids=["both slopes: x first, then y", "a negative x", "y alone", "x alone"],
)
def test_a_pixel_goes_where_the_two_shears_in_turn_take_it(x: float, y: float, moved_to: tuple[int, int]) -> None:
    """The pixel 20 rows below and 10 columns right of the centre (50, 50) is at offset (row 20, col 10). The skew by
    `x` first moves it x x 20 columns right; then the skew by `y` moves it y x its new column offset rows down:
    x 0.5, y 0.25: column 10 + 10 = 20, row 20 + 5 = 25, so the pixel (75, 70);
    x -0.5, y 0.25: column 10 - 10 = 0, row 20 + 0 = 20, so (70, 50);
    x 0, y 0.5: column 10, row 20 + 5 = 25, so (75, 60);
    x 0.4, y 0: column 10 + 8 = 18, row 20, so (70, 68)."""
    dot = np.zeros((101, 101, 3), np.float32)
    dot[70, 60] = 1.0
    out = registry.apply(NAME, dot, x=x, y=y, interpolation="nearest")[..., 0]
    assert np.argwhere(out == 1.0).tolist() == [list(moved_to)]
    assert out.sum() == 1.0


@pytest.mark.parametrize(
    ("interpolation", "power"),
    [("bilinear", 1), ("bicubic", 1), ("bicubic", 2)],
    ids=["bilinear reads a ramp exactly", "bicubic reads a ramp exactly", "bicubic reads a parabola exactly"],
)
@pytest.mark.parametrize(("x", "y"), [(0.4, 0.0), (0.0, -0.3), (0.35, 0.2), (-0.5, 0.45)])
def test_between_pixels_a_smooth_picture_is_read_where_the_two_shears_undone_in_turn_say(
    interpolation: str, power: int, x: float, y: float
) -> None:
    """A picture whose value is ((2 row + column) / 300)^power is a ramp, linear in both directions (power 1: read
    exactly by bilinear and, away from the edge, bicubic interpolation) or a parabola (power 2: only bicubic's cubic
    spline reads it exactly). Each pixel shows the picture at the point that the y skew and then the x skew take back to
    it: the shears of the pixel's offset from the centre (50, 50) undone in the opposite order."""
    rows, cols = np.mgrid[0:101, 0:101].astype(np.float64)
    out = registry.apply(NAME, grey(((2 * rows + cols) / 300.0) ** power), x=x, y=y, interpolation=interpolation)[
        ..., 0
    ]
    dr, dc = rows - 50, cols - 50
    before_y = dr - y * dc  # undo the later skew (rows down by y per column) ...
    before_x = dc - x * before_y  # ... then the earlier one (columns right by x per row)
    source_rows, source_cols = 50 + before_y, 50 + before_x
    inside = (source_rows > 15) & (source_rows < 85) & (source_cols > 15) & (source_cols < 85)
    assert inside.sum() > 2000
    expected = ((2 * source_rows + source_cols) / 300.0) ** power
    np.testing.assert_allclose(out[inside], expected[inside], atol=1e-6)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_shear_slides_it_in_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 columns whose value is column / 20, sheared by x 1: the pixel (row 0, column 20) is 10 rows above
    the centre row, so it shows the point 10 columns to its right, column 30, beyond the picture, where
    nearest: the last column, 1.0; mirror: column 10 (mirrored about column 20), 0.5; reflect: column 11 (mirrored
    about the edge of the last pixel, which repeats it), 0.55; wrap: column 9 (the picture repeated, 21
    columns), 0.45."""
    columns = np.tile(np.arange(21) / 20.0, (21, 1))
    out = registry.apply(NAME, grey(columns), x=1.0, y=0.0, edge=edge)
    assert out[0, 20, 0] == pytest.approx(expected, abs=1e-5)
