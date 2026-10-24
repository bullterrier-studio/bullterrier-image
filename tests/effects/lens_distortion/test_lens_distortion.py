"""Behaviour of the `lens_distortion` effect: radial barrel or pincushion distortion, with the picture scaled back by
`fit`."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "lens_distortion"
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]
FITS = ["frame", "centre"]
EDGES = ["reflect", "mirror", "nearest", "wrap"]


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_distortion_zero_leaves_the_picture_unchanged(fit: str, interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, distortion=0.0, fit=fit, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize("distortion", [0.6, -0.25])
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (0.25, 0.75, (45, 20)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "the top left pixel", "off centre", "the right edge"],
)
def test_the_centre_pixel_does_not_move(
    fit: str, distortion: float, x: float, y: float, pixel: tuple[int, int]
) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, distortion=distortion, fit=fit, x=x, y=y)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is bent


@pytest.mark.parametrize(
    ("distortion", "fit", "expected"),
    [
        (0.2, "centre", 0.86715),
        (-0.2, "centre", 0.83285),
        (0.2, "frame", 0.80596),
        (-0.2, "frame", 0.91606),
    ],
    ids=["barrel, centre kept", "pincushion, centre kept", "barrel, frame filled", "pincushion, frame filled"],
)
def test_a_pixel_shows_the_picture_at_the_radius_the_distortion_gives(
    distortion: float, fit: str, expected: float
) -> None:
    """On a picture of 101 x 101 px the farthest corner is R = 50 sqrt(2) = 70.71 px from the centre (50, 50); the
    pixel 35 px right of the centre is at the radius 35 / R = 0.495 of it, and reads the picture at the radius
    0.495 x (1 + distortion x 0.495^2) on the same ray (barrel 0.2: 0.5192 = 36.71 px, so a ramp column / 100 shows
    0.86715; pincushion -0.2: 0.4707 = 33.28 px, 0.83285), divided by 1 + distortion with fit 'frame' (0.2: 30.60 px,
    0.80596; -0.2: 41.61 px, 0.91606)."""
    ramp = np.tile(np.arange(101) / 100.0, (101, 1))
    out = registry.apply(NAME, grey(ramp), distortion=distortion, fit=fit, interpolation="bilinear")
    assert out[50, 85, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize("fit", FITS)
@pytest.mark.parametrize(("x", "y"), [(0.5, 0.5), (0.2, 0.9)], ids=["centred", "off centre"])
@pytest.mark.parametrize("distortion", [0.35, -0.2])
def test_every_pixel_shows_the_picture_at_the_radius_the_distortion_gives(
    fit: str, x: float, y: float, distortion: float
) -> None:
    """Brute force on a linear picture (read exactly between its pixels by bilinear interpolation): the pixel at
    distance d from the axis shows the ramp on the same ray at d x (1 + distortion x (d / R)^2), divided by 1 +
    distortion for 'frame', R the distance of the farthest of the four corners."""
    height, width = 31, 41
    axis_row, axis_col = y * (height - 1), x * (width - 1)
    farthest = max(math.hypot(r - axis_row, c - axis_col) for r in (0, height - 1) for c in (0, width - 1))
    rows, cols = np.mgrid[0:height, 0:width].astype(np.float64)
    ramp = (2 * rows + cols + 30) / 150.0
    out = registry.apply(NAME, grey(ramp), distortion=distortion, fit=fit, x=x, y=y, edge="nearest")[..., 0]
    expected = np.empty_like(ramp)
    inside = np.zeros_like(ramp, dtype=bool)  # leaves out what reads beyond the picture: that is the edge rule's
    for row in range(height):
        for col in range(width):
            scale = 1 + distortion * (math.hypot(row - axis_row, col - axis_col) / farthest) ** 2
            scale /= 1 + distortion if fit == "frame" else 1
            read_row, read_col = axis_row + (row - axis_row) * scale, axis_col + (col - axis_col) * scale
            expected[row, col] = (2 * read_row + read_col + 30) / 150.0
            inside[row, col] = 0 <= read_row <= height - 1 and 0 <= read_col <= width - 1
    assert inside.sum() > 0.6 * inside.size
    np.testing.assert_allclose(out[inside], expected[inside], atol=1e-5)


@pytest.mark.parametrize("distortion", [1.0, 0.4])
@pytest.mark.parametrize("edge", EDGES)
def test_a_barrel_with_fit_frame_reads_only_inside_the_picture_so_the_edge_rule_changes_nothing(
    distortion: float, edge: str
) -> None:
    noise = np.random.default_rng(5).random((37, 53, 3), dtype=np.float32)
    kept = registry.apply(NAME, noise, distortion=distortion, fit="frame", edge="reflect", x=0.3, y=0.7)
    out = registry.apply(NAME, noise, distortion=distortion, fit="frame", edge=edge, x=0.3, y=0.7)
    np.testing.assert_allclose(out, kept, atol=1e-5)


def test_a_pincushion_with_fit_frame_reads_beyond_the_edges_so_the_edge_rule_shows() -> None:
    """The farthest corner is kept, so the middle of an edge, which is nearer the axis, reads farther out than it
    shows (the pixel 50 px from the axis of 101 x 101 reads at 50 x (1 - 0.3 x 0.5) / 0.7 = 60.7 px: beyond it)."""
    noise = np.random.default_rng(6).random((101, 101, 3), dtype=np.float32)
    reflected = registry.apply(NAME, noise, distortion=-0.3, fit="frame", edge="reflect")
    wrapped = registry.apply(NAME, noise, distortion=-0.3, fit="frame", edge="wrap")
    assert np.abs(reflected - wrapped)[50, 100, 0] > 0.0
    assert np.abs(reflected - wrapped).max() > 0.1


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_barrel_with_the_centre_kept_sees_beyond_the_picture(edge: str, expected: float) -> None:
    """A picture of 21 columns with the value column / 20, the centre on its top left pixel, distortion 1: the
    farthest corner is (20, 20), R = 28.28 px away, so the pixel (0, 20) is at the radius 0.7071 and reads at 20 x
    (1 + 0.7071^2) = 30 px, 10 px beyond the last column. Nearest repeats the last column, 1.0; mirror reads column 10
    (mirrored about column 20), 0.5; reflect column 11 (mirrored about the edge of the last pixel, which it repeats),
    0.55; wrap column 9 (21 columns repeat), 0.45."""
    ramp = np.tile(np.arange(21) / 20.0, (21, 1))
    out = registry.apply(
        NAME, grey(ramp), distortion=1.0, fit="centre", x=0.0, y=0.0, edge=edge, interpolation="nearest"
    )
    assert out[0, 20, 0] == pytest.approx(expected, abs=1e-6)


def test_a_barrel_shrinks_the_middle_less_than_the_edge_and_a_pincushion_the_opposite() -> None:
    """A picture whose value is the distance from the centre comes out smaller than that near the middle for a barrel
    with the frame filled (the middle is magnified, each pixel reads from nearer the centre) and larger for a
    pincushion."""
    rows, cols = np.mgrid[0:101, 0:101].astype(np.float64) - 50
    distance = np.hypot(rows, cols) / (50 * math.sqrt(2))
    middle = (distance > 0.1) & (distance < 0.5)
    barrel = registry.apply(NAME, grey(distance), distortion=0.5, fit="frame")[..., 0]
    pincushion = registry.apply(NAME, grey(distance), distortion=-0.25, fit="frame")[..., 0]
    assert (barrel[middle] < distance[middle]).all()
    assert (pincushion[middle] > distance[middle]).all()
