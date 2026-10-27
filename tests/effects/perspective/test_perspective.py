"""Behaviour of the `perspective` effect: the picture tilted as a camera would see it, keystoned along each axis, skewed
(`shear_x`, `shear_y`), then turned and zoomed about its centre."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "perspective"
FLOAT32_TOLERANCE = 2 * float(
    np.finfo(np.float32).eps
)  # 2 x float32 epsilon: what one rounding of a float32 picture costs
NEUTRAL = {
    "vertical": 0.0,
    "horizontal": 0.0,
    "shear_x": 0.0,
    "shear_y": 0.0,
    "rotate": 0.0,
    "scale": 0.0,
}  # the default keystones; each test names its own


def perspective(picture: np.ndarray, **settings: float | str) -> np.ndarray:
    """The effect on `picture` with every correction at 0 except those in `settings`."""
    return registry.apply(NAME, picture, **{**NEUTRAL, **settings})


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


def blob_at(row: int, col: int, shape: tuple[int, int] = (101, 101)) -> np.ndarray:
    """A black picture of `shape` with a white 9 x 9 square centred on the pixel (row, col)."""
    picture = np.zeros((*shape, 3), np.float32)
    picture[row - 4 : row + 5, col - 4 : col + 5] = 1.0
    return picture


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_perspective_of_nothing_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = perspective(photo, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


SQUARE = (101, 101)
WIDE = (81, 121)


@pytest.mark.parametrize(
    ("settings", "shape", "spot", "lands_at"),
    [
        ({"vertical": 0.5}, SQUARE, (12, 22), (29.75, 35.08)),
        ({"vertical": -0.5}, SQUARE, (88, 22), (70.25, 35.08)),
        ({"horizontal": 0.5}, SQUARE, (12, 78), (28.62, 65.75)),
        ({"vertical": 0.5, "horizontal": 0.5}, SQUARE, (12, 22), (35.02, 38.96)),
        ({"vertical": 0.5, "rotate": 90.0}, SQUARE, (12, 22), (35.08, 70.25)),
        ({"vertical": 0.5, "scale": 0.5}, SQUARE, (12, 22), (19.62, 27.61)),
        ({"scale": 0.5}, SQUARE, (34, 40), (26.0, 35.0)),
        ({"scale": -0.5}, SQUARE, (10, 20), (30.0, 35.0)),
        ({"vertical": 0.5, "shear_y": 0.5}, SQUARE, (12, 22), (22.29, 35.08)),
        ({"shear_x": 0.5, "rotate": 90.0}, SQUARE, (70, 60), (70.0, 30.0)),
        ({"vertical": 0.5}, WIDE, (10, 32), (23.96, 45.03)),
        ({"horizontal": 0.5}, WIDE, (15, 100), (26.34, 81.85)),
    ],
    ids=[
        "vertical shrinks the top",
        "a negative vertical shrinks the bottom",
        "horizontal shrinks the right",
        "horizontal comes after vertical",
        "rotate comes after the keystones",
        "scale comes after the keystones",
        "scale 0.5 magnifies by 1.5",
        "scale -0.5 shrinks by half",
        "shear comes after the keystones",
        "shear comes before the turn",
        "vertical in a wide picture",
        "horizontal in a wide picture",
    ],
)
def test_a_marked_spot_lands_where_the_homography_worked_by_hand_says(
    settings: dict[str, float], shape: tuple[int, int], spot: tuple[int, int], lands_at: tuple[float, float]
) -> None:
    """A 9 x 9 white square centred on `spot` of a picture of `shape` lands, bilinear, with its middle at `lands_at`,
    worked by hand (the black edge pixels are repeated, so nothing else is white). In the 101 x 101 picture the centre
    pixel is (50, 50) and the picture's edges, at the pixel edges, are 50.5 px from it:
    - vertical 0.5 shrinks the top edge to half the width of the bottom one, which keeps its width, along a projection
      that keeps lines straight: X' = a X / w, Y' = a Y / w with w = 1 - Y / 151.5 and a = 2/3, so the spot at offset
      X -28, Y -38 lands at X' -14.92, Y' -20.25, which is pixel (29.75, 35.08); vertical -0.5 is the same
      at the bottom;
    - horizontal 0.5 does the same with the columns, shrinking the right edge: from X 28, Y -38 to pixel (28.62, 65.75);
    - both keystones, vertical first: pixel (35.02, 38.96);
    - then shear_y 0.5 slides the keystoned spot (X' -14.92, Y' -20.25) down 0.5 x -14.92 = -7.46 px: pixel
      (22.29, 35.08); a shear before the keystone would give (24.19, 36.1);
    - shear_x 0.5 slides the offset (X 10, Y 20) to column offset 10 + 10 = 20, which rotate 90 turns, (X, Y) to
      (-Y, X), to X' -20, Y' 20: pixel (70, 30); a turn before the shear would give (60, 35);
    - then rotate 90 turns the keystoned spot clockwise, (X, Y) to (-Y, X): X' 20.25, Y' -14.92, pixel (35.08, 70.25);
    - then scale 0.5 magnifies it by 1.5 about the centre: pixel (19.62, 27.61);
    - scale 0.5 alone takes the offset (-16, -10) to (-24, -15), scale -0.5 takes (-40, -30) to (-20, -15);
    - the 81 x 121 picture has half sizes 40.5 and 60.5 (centre pixel (40, 60)): the keystone depth w follows the half
      height, 1 - Y / 121.5, and the half width, 1 + X / 181.5, so the same arithmetic gives the last two rows."""
    out = perspective(blob_at(*spot, shape), interpolation="bilinear", edge="nearest", **settings)
    np.testing.assert_allclose(ndimage.center_of_mass(out[..., 0]), lands_at, atol=0.6)


@pytest.mark.parametrize(
    ("rotate", "quarter_turns"),
    [(90.0, -1), (-90.0, 1), (180.0, 2)],
    ids=["90 is a clockwise quarter turn", "-90 is counter-clockwise", "180 is a half turn"],
)
def test_rotate_turns_the_picture_clockwise_about_its_centre(rotate: float, quarter_turns: int) -> None:
    noise = np.random.default_rng(5).random((61, 61, 3), dtype=np.float32)
    out = perspective(noise, rotate=rotate, interpolation="nearest")
    np.testing.assert_array_equal(out, np.rot90(noise, quarter_turns))  # np.rot90 counts counter-clockwise turns


@pytest.mark.parametrize(
    ("settings", "mirrored_by", "flip"),
    [
        ({"vertical": 0.4}, {"vertical": -0.4}, "flipud"),
        ({"horizontal": 0.4}, {"horizontal": -0.4}, "fliplr"),
        (
            {"vertical": 0.3, "horizontal": -0.2, "rotate": 7.0},
            {"vertical": -0.3, "horizontal": -0.2, "rotate": -7.0},
            "flipud",
        ),
    ],
    ids=["a keystone flips with the picture", "likewise along the columns", "turn and keystones together"],
)
def test_a_mirrored_picture_is_keystoned_the_opposite_way(
    settings: dict[str, float], mirrored_by: dict[str, float], flip: str
) -> None:
    """Mirroring the picture, keystoning it one way and mirroring it back is keystoning it the other way (a mirror
    reverses which side is far and which way a turn goes)."""
    noise = np.random.default_rng(6).random((96, 128, 3), dtype=np.float32)
    mirror = getattr(np, flip)
    out = mirror(perspective(mirror(noise), **settings))
    np.testing.assert_allclose(out, perspective(noise, **mirrored_by), atol=1e-4)


def test_a_positive_horizontal_is_the_vertical_that_shrinks_the_right_side_instead_of_the_top() -> None:
    """Transposing a square picture swaps its right side with its bottom one, so a horizontal keystone of 0.4 (the right
    side far) is a vertical keystone of -0.4 (the bottom far) of the transposed picture, transposed back."""
    noise = np.random.default_rng(7).random((64, 64, 3), dtype=np.float32)
    out = perspective(noise, horizontal=0.4)
    through_vertical = perspective(noise.transpose(1, 0, 2).copy(), vertical=-0.4).transpose(1, 0, 2)
    np.testing.assert_allclose(out, through_vertical, atol=1e-4)
    assert np.abs(out - noise).max() > 0.2  # and the keystone really changes the picture


@pytest.mark.parametrize(
    ("interpolation", "power"),
    [("bilinear", 1), ("bicubic", 1), ("bicubic", 2)],
    ids=["bilinear reads a ramp exactly", "bicubic reads a ramp exactly", "bicubic reads a parabola exactly"],
)
@pytest.mark.parametrize(("rotate", "scale"), [(0.0, 0.5), (30.0, 0.0), (-50.0, -0.25), (15.0, 0.3)])
def test_turned_and_zoomed_a_smooth_picture_is_read_at_the_pixels_offset_turned_back_and_shrunk(
    interpolation: str, power: int, rotate: float, scale: float
) -> None:
    """A picture whose value is ((2 row + column) / 300)^power is a ramp, linear in both directions (power 1: read
    exactly by bilinear and, away from the edge, bicubic interpolation) or a parabola (power 2: only bicubic's cubic
    spline reads it exactly). Each pixel shows the picture at its offset from the centre (50, 50) divided by 1 + scale
    and turned counter-clockwise by `rotate` (a clockwise turn of the picture shows what was a turn behind)."""
    rows, cols = np.mgrid[0:101, 0:101].astype(np.float64)
    out = perspective(
        grey(((2 * rows + cols) / 300.0) ** power), rotate=rotate, scale=scale, interpolation=interpolation
    )[..., 0]
    dr, dc = (rows - 50) / (1 + scale), (cols - 50) / (1 + scale)
    angle = np.radians(rotate)
    source_rows = 50 + dr * np.cos(angle) - dc * np.sin(angle)  # the offset turned counter-clockwise by the angle:
    source_cols = 50 + dr * np.sin(angle) + dc * np.cos(angle)  # clockwise, as the swirl's, would be the other sign
    inside = (source_rows > 15) & (source_rows < 85) & (source_cols > 15) & (source_cols < 85)
    assert inside.sum() > 2000
    expected = ((2 * source_rows + source_cols) / 300.0) ** power
    np.testing.assert_allclose(out[inside], expected[inside], atol=1e-6)


def test_straight_lines_stay_straight() -> None:
    """An anti-aliased line 3 px wide across a 201 x 201 picture, keystoned both ways, turned and zoomed out, is still a
    line: the middle of its brightness in each row sits within 1 px of the straight line fitted through them."""
    rows, cols = np.mgrid[0:201, 0:201].astype(np.float64)
    # px from the line through (row 20, column 30) and (row 180, column 160)
    distance = np.abs(160 * (cols - 30) - 130 * (rows - 20)) / np.hypot(160, 130)
    line = grey(np.clip(2 - distance, 0, 1) * ((rows >= 20) & (rows <= 180)))
    out = perspective(line, vertical=0.4, horizontal=-0.3, rotate=10.0, scale=-0.2, edge="nearest")[..., 0]
    brightness = out.sum(axis=1)
    lit = np.flatnonzero(brightness > 0.5)
    assert len(lit) > 80  # a good stretch of the line is still there
    middles = (out[lit] * np.arange(201)).sum(axis=1) / brightness[lit]
    residual = middles - np.polyval(np.polyfit(lit, middles, 1), lit)
    assert np.abs(residual).max() < 1.0
    assert np.abs(np.polyfit(lit, middles, 1)[0] - 130 / 160) > 0.05  # and the slope really has changed


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.0), ("mirror", 0.931818), ("reflect", 0.881818), ("wrap", 0.118182)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_keystone_reads_it_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 rows whose value is row / 20, keystoned by vertical 0.5: a pixel at Y' from the centre row (half
    height 10.5) reads the point Y = (Y' / a) / (1 + Y' / (31.5 a)) with a = 2/3, the keystone worked backwards. The
    pixel at the top of the centre column, Y' = -10, shows Y = -15 / (11/21) = -28.64 px from the centre row, which is
    the point 18.64 px above the first row (row -18.64), where
    nearest: the first row, 0; mirror: row 18.64 (mirrored about row 0), 0.9318; reflect: row 17.64 (mirrored about the
    edge of the first pixel, which repeats it), 0.8818; wrap: row 2.36 (the picture repeated, 21 rows), 0.1182."""
    rows = np.tile((np.arange(21) / 20.0)[:, None], (1, 21))
    out = perspective(grey(rows), vertical=0.5, edge=edge)
    assert out[0, 10, 0] == pytest.approx(expected, abs=2e-5)


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
    """A white column `line_at` of a black 101 x 101 picture, centre row 50, sheared by `shear_x`: a pixel d rows below
    the centre row moves x x d columns right, so the line is a line of slope x that crosses column `line_at` at the
    centre row, and 'nearest' puts each of its pixels on the closest column. `y` is the same with rows and columns
    swapped: a white row, moved down y px per column right of the centre column."""
    line = np.zeros((101, 101, 3), np.float32)
    line[:, line_at] = 1.0
    picture = line if setting == "x" else line.transpose(1, 0, 2).copy()
    out = perspective(picture, **{f"shear_{setting}": slope}, interpolation="nearest")
    lit = out[..., 0] if setting == "x" else out[..., 0].T  # in both cases, one lit pixel per row, found in its columns
    expected = np.floor(line_at + slope * (np.arange(101) - 50) + 0.5)
    assert set(np.unique(out)) == {0.0, 1.0}  # 'nearest' keeps the picture's own values
    assert (lit.sum(axis=1) == 1).all()  # one white pixel in every row
    np.testing.assert_array_equal(lit.argmax(axis=1), expected)


@pytest.mark.parametrize(
    ("x", "y", "moved_to"),
    [(0.4, 1 / 3, (76, 68)), (-0.4, 2.0, (74, 52)), (0.0, 2.0, (90, 60)), (0.4, 0.0, (70, 68))],
    ids=["both slopes: x first, then y", "a negative x", "y alone", "x alone"],
)
def test_a_pixel_goes_where_the_two_shears_in_turn_take_it(x: float, y: float, moved_to: tuple[int, int]) -> None:
    """The pixel 20 rows below and 10 columns right of the centre (50, 50) is at offset (row 20, col 10). The skew by
    `shear_x` first moves it x x 20 columns right; then the skew by `shear_y` moves it y x its new column offset rows
    down (the slopes are chosen so that no pixel of the output reads exactly half way between two source pixels):
    x 0.4, y 1/3: column 10 + 8 = 18, row 20 + 6 = 26, so the pixel (76, 68);
    x -0.4, y 2: column 10 - 8 = 2, row 20 + 4 = 24, so (74, 52);
    x 0, y 2: column 10, row 20 + 20 = 40, so (90, 60);
    x 0.4, y 0: column 10 + 8 = 18, row 20, so (70, 68)."""
    dot = np.zeros((101, 101, 3), np.float32)
    dot[70, 60] = 1.0
    out = perspective(dot, shear_x=x, shear_y=y, interpolation="nearest", edge="nearest")[..., 0]
    assert np.argwhere(out == 1.0).tolist() == [list(moved_to)]
    assert out.sum() == 1.0


@pytest.mark.parametrize("setting", ["x", "y"])
@pytest.mark.parametrize("slope", [0.3, -0.45, 0.6])
def test_bilinear_a_line_across_the_shear_is_a_tent_centred_on_the_line_of_that_slope(
    setting: str, slope: float
) -> None:
    """A white column 40 of a black 101 x 101 picture, sheared by `shear_x` and read bilinear: in each row the
    brightness is a tent, 1 - |column - p| for columns within 1 of p = 40 + slope x (rows below the centre row 50), the
    two-pixel blend of a line at p. `shear_y` is the same with rows and columns swapped."""
    line = np.zeros((101, 101, 3), np.float32)
    line[:, 40] = 1.0
    picture = line if setting == "x" else line.transpose(1, 0, 2).copy()
    out = perspective(picture, **{f"shear_{setting}": slope}, interpolation="bilinear")[..., 0]
    seen = out if setting == "x" else out.T  # in both cases, the line runs down the rows
    centre = 40 + slope * (np.arange(101) - 50)
    expected = np.clip(1 - np.abs(np.arange(101)[None, :] - centre[:, None]), 0, 1)
    np.testing.assert_allclose(seen, expected, atol=FLOAT32_TOLERANCE)


@pytest.mark.parametrize("edge", ["reflect", "nearest", "mirror", "wrap"])
@pytest.mark.parametrize(
    "settings",
    [
        {"shear_x": 0.7},
        {"shear_y": -1.3},
        {"shear_x": 2.0, "shear_y": -2.0},
        {"vertical": 0.4, "shear_x": 0.3, "rotate": 20.0},
    ],
    ids=["x", "y", "both at their limits", "with a keystone and a turn"],
)
def test_nearest_every_output_value_is_a_value_of_the_input(edge: str, settings: dict[str, float]) -> None:
    noise = np.random.default_rng(8).random((48, 64, 3), dtype=np.float32)
    out = perspective(noise, interpolation="nearest", edge=edge, **settings)
    assert np.isin(out, noise).all()


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
    out = perspective(grey(((2 * rows + cols) / 300.0) ** power), shear_x=x, shear_y=y, interpolation=interpolation)[
        ..., 0
    ]
    dr, dc = rows - 50, cols - 50
    before_y = dr - y * dc  # undo the later skew (rows down by y per column) ...
    before_x = dc - x * before_y  # ... then the earlier one (columns right by x per row)
    source_rows, source_cols = 50 + before_y, 50 + before_x
    inside = (source_rows > 15) & (source_rows < 85) & (source_cols > 15) & (source_cols < 85)
    assert inside.sum() > 2000
    expected = ((2 * source_rows + source_cols) / 300.0) ** power
    np.testing.assert_allclose(out[inside], expected[inside], atol=FLOAT32_TOLERANCE)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_shear_slides_it_in_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 columns whose value is column / 20, sheared by shear_x 1: the pixel (row 0, column 20) is 10 rows
    above the centre row, so it shows the point 10 columns to its right, column 30, beyond the picture, where
    nearest: the last column, 1.0; mirror: column 10 (mirrored about column 20), 0.5; reflect: column 11 (mirrored
    about the edge of the last pixel, which repeats it), 0.55; wrap: column 9 (the picture repeated, 21
    columns), 0.45."""
    columns = np.tile(np.arange(21) / 20.0, (21, 1))
    out = perspective(grey(columns), shear_x=1.0, edge=edge)
    assert out[0, 20, 0] == pytest.approx(expected, abs=FLOAT32_TOLERANCE)
