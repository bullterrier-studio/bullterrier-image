"""Behaviour of the `lens_distortion` effect: radial barrel or pincushion distortion by the Brown-Conrady polynomial or
a fisheye curve, with the picture scaled back by `fit`."""

import math
from collections.abc import Callable

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "lens_distortion"
MODELS = ["brown_conrady", "fisheye"]
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]
FITS = ["frame", "centre"]
EDGES = ["reflect", "mirror", "nearest", "wrap"]
# How far from the axis a pixel's source lies, as a multiple of the pixel's own distance, at the radius r (the pixel's
# distance over the farthest corner's), for a few lenses worked out by hand: (model, amount, that curve).
# A fisheye of half field of view w is tan(w r) / (w r), whose value at the corner r = 1 is tan(w) / w, so its amount
# is tan(w) / w - 1; its inverse for a negative amount is atan(v r) / (v r), with amount atan(v) / v - 1.
LENSES = [
    pytest.param("brown_conrady", 0.35, lambda r: 1 + 0.35 * r**2, id="barrel polynomial"),
    pytest.param("brown_conrady", -0.2, lambda r: 1 - 0.2 * r**2, id="pincushion polynomial"),
    pytest.param(
        "fisheye",
        math.tan(math.radians(45)) / math.radians(45) - 1,
        lambda r: math.tan(math.radians(45) * r) / (math.radians(45) * r) if r > 0 else 1.0,
        id="fisheye of a 90 degree lens",
    ),
    pytest.param(
        "fisheye",
        math.atan(1.2) / 1.2 - 1,
        lambda r: math.atan(1.2 * r) / (1.2 * r) if r > 0 else 1.0,
        id="inverse fisheye (arctangent)",
    ),
]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("model", MODELS)
@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_amount_zero_leaves_the_picture_unchanged(model: str, fit: str, interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, model=model, amount=0.0, fit=fit, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize("model", MODELS)
@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize("amount", [0.6, -0.25])
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (0.25, 0.75, (45, 20)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "the top left pixel", "off centre", "the right edge"],
)
def test_the_centre_pixel_does_not_move(
    model: str, fit: str, amount: float, x: float, y: float, pixel: tuple[int, int]
) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, model=model, amount=amount, fit=fit, x=x, y=y)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is bent


@pytest.mark.parametrize("model", MODELS)
def test_with_fit_centre_the_farthest_corner_reads_1_plus_amount_times_as_far_out_as_it_is(model: str) -> None:
    """Whatever the curve, `amount` is the stretch at the farthest corner. On 101 x 101 px with the amount -0.25 the
    corner (0, 100), 50 px up and right of the centre (50, 50), reads the picture 0.75 as far out, at (12.5, 87.5):
    on a ramp of column / 100, 0.875."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), model=model, amount=-0.25, fit="centre", interpolation="bilinear")
    assert out[0, 100, 0] == pytest.approx(0.875, abs=1e-5)


@pytest.mark.parametrize("model", MODELS)
@pytest.mark.parametrize("amount", [1.0, -0.25])
def test_with_fit_frame_the_farthest_corner_stays_and_the_middle_is_read_1_plus_amount_times_nearer(
    model: str, amount: float
) -> None:
    """Fit 'frame' divides the stretch by 1 + amount. The corner (0, 100) of 101 x 101 px stays; the pixel 1 px right
    of the centre, whose own curve is flat to within 1e-3 so near the axis, reads 1 / (1 + amount) px right of it."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), model=model, amount=amount, fit="frame", interpolation="bilinear")
    assert out[0, 100, 0] == pytest.approx(1.0, abs=1e-5)
    assert out[50, 51, 0] == pytest.approx(0.5 + 0.01 / (1 + amount), abs=2e-4)


@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize(("model", "amount", "curve"), LENSES)
@pytest.mark.parametrize(("x", "y"), [(0.5, 0.5), (0.2, 0.9)], ids=["centred", "off centre"])
def test_every_pixel_shows_the_picture_at_the_radius_the_lens_gives(
    model: str, amount: float, curve: Callable[[float], float], fit: str, x: float, y: float
) -> None:
    """Brute force on a linear picture (read exactly between its pixels by bilinear interpolation): the pixel at
    distance d from the axis shows the ramp on the same ray at d x curve(d / R), divided by 1 + amount for 'frame', R
    the distance of the farthest of the four corners."""
    height, width = 31, 41
    axis_row, axis_col = y * (height - 1), x * (width - 1)
    farthest = max(math.hypot(r - axis_row, c - axis_col) for r in (0, height - 1) for c in (0, width - 1))
    rows, cols = np.mgrid[0:height, 0:width].astype(np.float64)
    ramp = (2 * rows + cols + 30) / 150.0
    out = registry.apply(NAME, grey(ramp), model=model, amount=amount, fit=fit, x=x, y=y, edge="nearest")[..., 0]
    expected = np.empty_like(ramp)
    inside = np.zeros_like(ramp, dtype=bool)  # leaves out what reads beyond the picture: that is the edge rule's
    for row in range(height):
        for col in range(width):
            scale = curve(math.hypot(row - axis_row, col - axis_col) / farthest)
            scale /= 1 + amount if fit == "frame" else 1
            read_row, read_col = axis_row + (row - axis_row) * scale, axis_col + (col - axis_col) * scale
            expected[row, col] = (2 * read_row + read_col + 30) / 150.0
            inside[row, col] = 0 <= read_row <= height - 1 and 0 <= read_col <= width - 1
    assert inside.sum() > 0.6 * inside.size
    np.testing.assert_allclose(out[inside], expected[inside], atol=1e-5)


@pytest.mark.parametrize(
    ("amount", "fit", "expected"),
    [
        (0.2, "centre", 0.86715),
        (-0.2, "centre", 0.83285),
        (0.2, "frame", 0.80596),
        (-0.2, "frame", 0.91606),
    ],
    ids=["barrel, centre kept", "pincushion, centre kept", "barrel, frame filled", "pincushion, frame filled"],
)
def test_a_pixel_shows_the_picture_at_the_radius_the_polynomial_gives(amount: float, fit: str, expected: float) -> None:
    """On a picture of 101 x 101 px the farthest corner is R = 50 sqrt(2) = 70.71 px from the centre (50, 50); the
    pixel 35 px right of the centre is at the radius 35 / R = 0.495 of it, and reads the picture at the radius
    0.495 x (1 + amount x 0.495^2) on the same ray (barrel 0.2: 0.5192 = 36.71 px, so a ramp column / 100 shows
    0.86715; pincushion -0.2: 0.4707 = 33.28 px, 0.83285), divided by 1 + amount with fit 'frame' (0.2: 30.60 px,
    0.80596; -0.2: 41.61 px, 0.91606)."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), amount=amount, fit=fit, interpolation="bilinear")
    assert out[50, 85, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize(
    ("half_angle", "distance", "expected"),
    [(45, 35, 0.78963), (45, 20, 0.65972), (81, 35, 0.59428)],
    ids=["a 90 degree lens, far out", "a 90 degree lens, nearer", "a 162 degree lens"],
)
def test_a_fisheye_pixel_shows_the_picture_at_the_distance_the_lens_maps_it_from(
    half_angle: int, distance: int, expected: float
) -> None:
    """An equidistant fisheye of half field of view w has the amount tan(w) / w - 1. On a picture of 101 x 101 px the
    farthest corner is 50 sqrt(2) = 70.71 px from the centre (50, 50), and a pixel at distance d from it reads the
    picture at 70.71 x tan(d / 70.71 x w) / tan(w): for the pixel 35 px right of the centre and w = 45 degrees,
    70.71 x tan(0.4950 x 45 degrees) / 1 = 28.96 px right of it, so a ramp column / 100 shows (50 + 28.96) / 100 =
    0.78963; likewise 0.65972 for 20 px, and with w = 81 degrees for 35 px, 0.59428."""
    w = math.radians(half_angle)
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(
        NAME, grey(ramp), model="fisheye", amount=math.tan(w) / w - 1, fit="frame", interpolation="bilinear"
    )
    assert out[50, 50 + distance, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize("model", MODELS)
@pytest.mark.parametrize("amount", [1.0, 0.4])
@pytest.mark.parametrize("edge", EDGES)
def test_a_barrel_with_fit_frame_reads_only_inside_the_picture_so_the_edge_rule_changes_nothing(
    model: str, amount: float, edge: str
) -> None:
    noise = np.random.default_rng(5).random((37, 53, 3), dtype=np.float32)
    kept = registry.apply(NAME, noise, model=model, amount=amount, fit="frame", edge="reflect", x=0.3, y=0.7)
    out = registry.apply(NAME, noise, model=model, amount=amount, fit="frame", edge=edge, x=0.3, y=0.7)
    np.testing.assert_allclose(out, kept, atol=1e-5)


@pytest.mark.parametrize("model", MODELS)
def test_a_pincushion_with_fit_frame_reads_beyond_the_edges_so_the_edge_rule_shows(model: str) -> None:
    """The farthest corner is kept, so the middle of an edge, which is nearer the axis, reads farther out than it
    shows (for the polynomial, the pixel 50 px from the axis of 101 x 101 reads at 50 x (1 - 0.3 x 0.5) / 0.7 = 60.7
    px: beyond it)."""
    noise = np.random.default_rng(6).random((101, 101, 3), dtype=np.float32)
    reflected = registry.apply(NAME, noise, model=model, amount=-0.3, fit="frame", edge="reflect")
    wrapped = registry.apply(NAME, noise, model=model, amount=-0.3, fit="frame", edge="wrap")
    assert np.abs(reflected - wrapped)[50, 100, 0] > 0.0
    assert np.abs(reflected - wrapped).max() > 0.1


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_barrel_with_the_centre_kept_sees_beyond_the_picture(edge: str, expected: float) -> None:
    """A picture of 21 columns with the value column / 20, the centre on its top left pixel, amount 1: the
    farthest corner is (20, 20), R = 28.28 px away, so the pixel (0, 20) is at the radius 0.7071 and reads at 20 x
    (1 + 0.7071^2) = 30 px, 10 px beyond the last column. Nearest repeats the last column, 1.0; mirror reads column 10
    (mirrored about column 20), 0.5; reflect column 11 (mirrored about the edge of the last pixel, which it repeats),
    0.55; wrap column 9 (21 columns repeat), 0.45."""
    ramp = np.tile(np.arange(21) / 20.0, (21, 1))
    out = registry.apply(NAME, grey(ramp), amount=1.0, fit="centre", x=0.0, y=0.0, edge=edge, interpolation="nearest")
    assert out[0, 20, 0] == pytest.approx(expected, abs=1e-6)


@pytest.mark.parametrize("model", MODELS)
def test_a_barrel_magnifies_the_middle_and_a_pincushion_does_the_opposite(model: str) -> None:
    """A picture whose value is the distance from the centre comes out smaller than that near the middle for a barrel
    with the frame filled (the middle is magnified, each pixel reads from nearer the centre, and from nowhere farther
    out than where it is shown) and larger for a pincushion; along a row it still rises, so nothing is folded over."""
    rows, cols = np.mgrid[0:101, 0:101].astype(np.float64) - 50
    distance = np.hypot(rows, cols) / (50 * math.sqrt(2))
    middle = (distance > 0.1) & (distance < 0.5)
    barrel = registry.apply(NAME, grey(distance), model=model, amount=0.5, fit="frame")[..., 0]
    pincushion = registry.apply(NAME, grey(distance), model=model, amount=-0.25, fit="frame")[..., 0]
    assert (barrel[middle] < distance[middle]).all()
    assert (barrel <= distance + 1e-4).all()
    assert (np.diff(barrel[50, 50:]) > 0).all()
    assert (pincushion[middle] > distance[middle]).all()


def test_a_fisheye_leaves_the_middle_flatter_than_a_polynomial_of_the_same_amount() -> None:
    """Both stretch the farthest corner by the same amount, but the fisheye bends hardest near it: along a row at
    10 px and at 45 px from the axis of 101 x 101 px (radius 0.141 and 0.636) its curve is 1.0092 and 1.2317 against
    the polynomial's 1.02 and 1.405 (amount 1, before the division by 2 of fit 'frame'), so it reads nearer the axis."""
    ramp = grey(np.tile(np.arange(101) / 100.0, (101, 1)))
    polynomial = registry.apply(NAME, ramp, model="brown_conrady", amount=1.0, fit="frame", interpolation="bilinear")
    fisheye = registry.apply(NAME, ramp, model="fisheye", amount=1.0, fit="frame", interpolation="bilinear")
    assert fisheye[50, 60, 0] == pytest.approx(0.5 + 0.1 * 1.0092 / 2, abs=1e-4)
    assert polynomial[50, 60, 0] == pytest.approx(0.5 + 0.1 * 1.02 / 2, abs=1e-4)
    assert fisheye[50, 95, 0] == pytest.approx(0.5 + 0.45 * 1.2317 / 2, abs=1e-3)
    assert polynomial[50, 95, 0] == pytest.approx(0.5 + 0.45 * 1.405 / 2, abs=1e-3)
