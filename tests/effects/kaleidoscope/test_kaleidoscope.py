"""Behaviour of the `kaleidoscope` effect: the picture folded about a centre into `segments` repeated sectors, each a
wedge of the picture and its mirror image."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "kaleidoscope"


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


def noise(height: int, width: int, seed: int = 8) -> np.ndarray:
    """A picture in which every pixel is different."""
    return np.random.default_rng(seed).random((height, width, 3), dtype=np.float32)


@pytest.mark.parametrize("angle", [0.0, 17.0, -100.0])
@pytest.mark.parametrize("segments", [4, 8])
def test_a_picture_with_four_or_eight_segments_looks_the_same_turned_a_quarter(segments: int, angle: float) -> None:
    """Every sector is the same, so turning a square picture about its centre by a whole number of sectors (a quarter
    turn is 1 sector of 4 and 2 of 8) gives the same picture, whatever the angle."""
    out = registry.apply(NAME, noise(61, 61), segments=segments, angle=angle, interpolation="bilinear")
    np.testing.assert_allclose(np.rot90(out), out, atol=1e-4)
    assert np.abs(out - noise(61, 61)).max() > 0.2  # and the picture really is folded


@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.25, 0.75, (45, 20)), (0.0, 0.0, (0, 0)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "off centre", "the top left pixel", "the right edge"],
)
def test_the_centre_pixel_does_not_move(x: float, y: float, pixel: tuple[int, int]) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    picture = noise(61, 81)
    out = registry.apply(NAME, picture, segments=5, angle=30.0, x=x, y=y, interpolation="nearest")
    np.testing.assert_allclose(out[pixel], picture[pixel], atol=1e-6)
    assert np.abs(out - picture).max() > 0.2  # and the rest of the picture is folded


@pytest.mark.parametrize(
    ("segments", "flips"),
    [(2, ["flipud", "fliplr"]), (4, ["flipud", "fliplr"]), (6, ["flipud", "fliplr"]), (3, ["flipud"]), (5, ["flipud"])],
    ids=["2 segments", "4 segments", "6 segments", "3 segments: only the 0 degree axis", "5 segments: likewise"],
)
def test_the_picture_is_symmetric_across_each_mirror_axis_through_the_centre(segments: int, flips: list[str]) -> None:
    """With angle 0 a mirror axis points right, along the middle row, and with an even count another points down
    (the axes are every 180 / segments degrees): reflecting an odd-sized picture across its middle row, or column,
    changes nothing."""
    out = registry.apply(NAME, noise(61, 61), segments=segments, angle=0.0, interpolation="bilinear")
    for flip in flips:
        np.testing.assert_allclose(getattr(np, flip)(out), out, atol=1e-4)
    assert np.abs(out - noise(61, 61)).max() > 0.2  # and the picture really is folded


def test_a_square_picture_of_four_segments_is_symmetric_across_its_diagonals() -> None:
    out = registry.apply(NAME, noise(61, 61), segments=4, angle=0.0, interpolation="bilinear")
    np.testing.assert_allclose(out.transpose(1, 0, 2), out, atol=1e-4)  # the diagonal at 45 degrees is an axis


@pytest.mark.parametrize(
    ("segments", "angle", "from_degrees", "to_degrees"),
    [(4, 0.0, 1.0, 44.0), (6, 20.0, 21.0, 49.0), (3, -90.0, -89.0, -31.0)],
    ids=["the first wedge of 4 segments", "a wedge turned by 20 degrees", "a negative angle"],
)
def test_the_wedge_that_follows_the_angle_shows_the_picture_as_it_is(
    segments: int, angle: float, from_degrees: float, to_degrees: float
) -> None:
    """The first half of each sector, from `angle` clockwise (rows run down) for 180 / segments degrees, is the
    picture itself; only the other half is its mirror image. Checked a degree inside each end of the wedge."""
    picture = noise(81, 81)
    out = registry.apply(NAME, picture, segments=segments, angle=angle, interpolation="bilinear")
    rows, cols = np.mgrid[0:81, 0:81] - 40
    degrees = np.degrees(np.arctan2(rows, cols))  # clockwise from pointing right, -180 to 180
    in_wedge = (((degrees - from_degrees) % 360) <= (to_degrees - from_degrees)) & (np.hypot(rows, cols) > 3)
    assert in_wedge.sum() > 300
    np.testing.assert_allclose(out[in_wedge], picture[in_wedge], atol=1e-4)
    assert np.abs(out - picture)[~in_wedge].max() > 0.2  # and the rest of the picture is not


@pytest.mark.parametrize(
    ("interpolation", "power"),
    [("bilinear", 1), ("bicubic", 1), ("bicubic", 2)],
    ids=["bilinear reads a ramp exactly", "bicubic reads a ramp exactly", "bicubic reads a parabola exactly"],
)
@pytest.mark.parametrize(
    ("segments", "angle", "x", "y"),
    [(5, 12.0, 0.3, 0.6), (2, -70.0, 0.5, 0.5), (7, 160.0, 0.6, 0.4), (3, 0.0, 0.5, 0.5)],
)
def test_between_pixels_each_pixel_shows_the_ramp_at_the_point_found_by_folding_its_angle(
    interpolation: str, power: int, segments: int, angle: float, x: float, y: float
) -> None:
    """A picture whose value is ((2 row + column) / 300)^power is a ramp, linear in both directions (power 1: read
    exactly by bilinear and, away from the edge, bicubic interpolation) or a parabola (power 2: only bicubic's cubic
    spline reads it exactly). A brute-force reference, pixel by pixel: the pixel's angle about the centre (clockwise
    from pointing right) less `angle`, taken modulo one sector (360 / segments degrees); a remainder past half a sector
    is replaced by what it falls short of the whole sector by; and the pixel shows the picture at the same distance
    from the centre, at `angle` plus that remainder."""
    height, width = 61, 81
    rows, cols = np.mgrid[0:height, 0:width].astype(np.float64)
    out = registry.apply(
        NAME,
        grey(((2 * rows + cols) / 300.0) ** power),
        segments=segments,
        angle=angle,
        x=x,
        y=y,
        interpolation=interpolation,
    )[..., 0]
    centre_row, centre_col = y * (height - 1), x * (width - 1)
    sector = 2 * math.pi / segments
    checked = 0
    for row in range(height):
        for col in range(width):
            dr, dc = row - centre_row, col - centre_col
            remainder = (math.atan2(dr, dc) - math.radians(angle)) % sector
            folded = min(remainder, sector - remainder)
            source_row = centre_row + math.hypot(dr, dc) * math.sin(math.radians(angle) + folded)
            source_col = centre_col + math.hypot(dr, dc) * math.cos(math.radians(angle) + folded)
            if 15 < source_row < height - 16 and 15 < source_col < width - 16:
                checked += 1
                expected = ((2 * source_row + source_col) / 300.0) ** power
                assert out[row, col] == pytest.approx(expected, abs=1e-5), (row, col)
    assert checked > 800


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.792893), ("reflect", 0.842893), ("wrap", 0.157107)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_fold_reads_it_from_beyond_the_picture(edge: str, expected: float) -> None:
    """A picture of 21 columns whose value is column / 20, folded in 8 segments (a sector is 45 degrees): the top left
    pixel (row 0, column 0) is 10 sqrt 2 = 14.14 px from the centre (10, 10) at -135 degrees, a whole number of sectors,
    so it shows the picture at that distance at 0 degrees: the point 14.14 px right of the centre, column 24.14, beyond
    the picture, where
    nearest: the last column, 1.0; mirror: column 15.86 (mirrored about column 20), 0.7929; reflect: column 16.86
    (mirrored about the edge of the last pixel, which repeats it), 0.8429; wrap: column 3.14 (the picture repeated, 21
    columns), 0.1571."""
    columns = np.tile(np.arange(21) / 20.0, (21, 1))
    out = registry.apply(NAME, grey(columns), segments=8, angle=0.0, edge=edge, interpolation="bilinear")
    assert out[0, 0, 0] == pytest.approx(expected, abs=1e-5)
