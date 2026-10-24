"""Behaviour of the `pinch_bulge` effect: the picture pushed out (bulge) or in (pinch) about a centre."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "pinch_bulge"
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize(
    "settings",
    [{"amount": 0.0}, {"radius": 0.0}],
    ids=["amount 0 moves nothing", "radius 0 reaches no pixel"],
)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_no_amount_or_no_reach_leaves_the_picture_unchanged(
    settings: dict[str, float], interpolation: str, photo: np.ndarray
) -> None:
    out = registry.apply(NAME, photo, interpolation=interpolation, **settings)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize("amount", [1.0, -1.0])
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (0.25, 0.75, (45, 20)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "the top left pixel", "off centre", "the right edge"],
)
def test_the_centre_pixel_does_not_move(
    amount: float, interpolation: str, x: float, y: float, pixel: tuple[int, int]
) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, amount=amount, radius=1.5, x=x, y=y, interpolation=interpolation)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is pushed


@pytest.mark.parametrize("amount", [0.8, -0.8])
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_pixels_at_or_beyond_the_radius_are_not_touched(amount: float, interpolation: str) -> None:
    """The radius is a fraction of the shorter side, first pixel to last: 0.3 x 60 = 18 px about pixel (30, 30)."""
    noise = np.random.default_rng(3).random((61, 61, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, amount=amount, radius=0.3, interpolation=interpolation)
    rows, cols = np.mgrid[0:61, 0:61] - 30
    outside = np.hypot(rows, cols) >= 18
    np.testing.assert_allclose(out[outside], noise[outside], atol=1e-6)
    assert np.abs(out - noise)[~outside].max() > 0.05


@pytest.mark.parametrize(
    ("amount", "expected"),
    [(0.5, 0.664), (-0.5, 0.736), (1.0, 0.5 + 0.2 * (1 - 0.6**2)), (-1.0, 0.5 + 0.2 * (1 + 0.6**2))],
    ids=["a bulge reads nearer the centre", "a pinch reads farther out", "a full bulge", "a full pinch"],
)
def test_a_pixel_shows_the_picture_at_the_distance_the_falloff_gives(amount: float, expected: float) -> None:
    """Radius 0.5 of 100 px is 50 px, so the pixel 20 px right of the centre (50, 50) is at 0.4 of it; the source
    lies at distance 20 x (1 - amount x (1 - 0.4)^2) on the same ray: for amount 0.5, 20 x 0.82 = 16.4 px right of
    the centre; for -0.5, 20 x 1.18 = 23.6 px. On a ramp column / 100 that pixel shows (50 + 16.4) / 100 = 0.664
    and (50 + 23.6) / 100 = 0.736 (and 0.5 + 0.2 x (1 -+ 0.36) for the full amounts)."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), amount=amount, radius=0.5, interpolation="bilinear")
    assert out[50, 70, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize("interpolation", ["bilinear", "bicubic"])
@pytest.mark.parametrize("amount", [0.7, -0.4])
def test_every_pixel_shows_the_picture_at_the_source_the_falloff_gives(interpolation: str, amount: float) -> None:
    """Brute force on a linear picture, read exactly between its pixels: each pixel at distance d (inside the radius
    R = 18 px) shows the ramp at the centre (30, 30) plus its offset times 1 - amount x (1 - d / R)^2."""
    rows, cols = np.mgrid[0:61, 0:61].astype(np.float64)
    ramp = (2 * rows + cols + 60) / 240.0
    out = registry.apply(NAME, grey(ramp), amount=amount, radius=0.3, interpolation=interpolation)[..., 0]
    expected = np.empty_like(ramp)
    for row in range(61):
        for col in range(61):
            dr, dc = row - 30.0, col - 30.0
            d = float(np.hypot(dr, dc))
            scale = 1 - amount * (1 - d / 18) ** 2 if d < 18 else 1.0
            expected[row, col] = (2 * (30 + dr * scale) + (30 + dc * scale) + 60) / 240.0
    np.testing.assert_allclose(out, expected, atol=1e-5)


@pytest.mark.parametrize(
    ("amount", "direction"),
    [(0.8, 1), (-0.8, -1)],
    ids=["a bulge enlarges the middle", "a pinch shrinks the middle"],
)
def test_the_sign_of_amount_pushes_the_middle_out_or_in(amount: float, direction: int) -> None:
    """A white square 10 px wide at the centre of a dark picture takes up more pixels bulged, fewer pinched."""
    square = np.zeros((101, 101, 3), np.float32)
    square[45:55, 45:55] = 1.0
    out = registry.apply(NAME, square, amount=amount, radius=0.5, interpolation="nearest")
    assert ((out[..., 0] > 0.5).sum() - 100) * direction > 20  # 100 white pixels before


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.75), ("reflect", 0.8), ("wrap", 0.2)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_pinch_reads_it_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 columns with the value column / 20, pinched about its top left pixel with radius 2 (40 px) and
    amount -1: the pixel (0, 20), 20 px from the centre, reads at 20 x (1 + (1 - 20 / 40)^2) = 25 px, 5 px right of
    the last column: nearest repeats the last column, 1.0; mirror reads column 15 (about column 20), 0.75; reflect
    column 16 (about the edge of the last pixel, which it repeats), 0.8; wrap column 4 (21 columns repeat), 0.2."""
    ramp = np.tile(np.arange(21) / 20.0, (21, 1))
    out = registry.apply(NAME, grey(ramp), amount=-1.0, radius=2.0, x=0.0, y=0.0, edge=edge, interpolation="nearest")
    assert out[0, 20, 0] == pytest.approx(expected, abs=1e-6)


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_a_constant_picture_stays_as_it_is(edge: str, interpolation: str) -> None:
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, amount=-1.0, radius=2.0, x=0.2, y=0.9, edge=edge, interpolation=interpolation)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_nearest_only_moves_whole_pixels_where_bilinear_blends_them() -> None:
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    nearest = registry.apply(NAME, noise, amount=0.6, radius=0.8, interpolation="nearest")
    bilinear = registry.apply(NAME, noise, amount=0.6, radius=0.8, interpolation="bilinear")
    assert np.isin(nearest, noise).all()
    assert not np.isin(bilinear, noise).all()
