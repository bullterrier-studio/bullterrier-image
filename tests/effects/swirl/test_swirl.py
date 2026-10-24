"""Behaviour of the `swirl` effect: the picture twisted about a centre, most at the centre, none at the `radius`."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "swirl"


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


def impulse(size: int, row: int, col: int) -> np.ndarray:
    """A black square picture with one white pixel."""
    dark = np.zeros((size, size, 3), np.float32)
    dark[row, col] = 1.0
    return dark


@pytest.mark.parametrize(
    "settings",
    [{"angle": 0.0}, {"radius": 0.0}],
    ids=["angle 0 turns nothing", "radius 0 reaches no pixel"],
)
@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_swirl_of_no_angle_or_no_reach_leaves_the_picture_unchanged(
    settings: dict[str, float], interpolation: str, photo: np.ndarray
) -> None:
    out = registry.apply(NAME, photo, interpolation=interpolation, **settings)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (0.25, 0.75, (45, 20)), (1.0, 0.5, (30, 80))],
    ids=["the middle", "the top left pixel", "off centre", "the right edge"],
)
def test_the_centre_pixel_does_not_move(interpolation: str, x: float, y: float, pixel: tuple[int, int]) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, angle=300.0, radius=1.5, x=x, y=y, interpolation=interpolation)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is twisted


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_pixels_at_or_beyond_the_radius_are_not_touched(interpolation: str) -> None:
    """The radius is a fraction of the shorter side, first pixel to last: 0.3 x 60 = 18 px about pixel (30, 30)."""
    noise = np.random.default_rng(3).random((61, 61, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, angle=500.0, radius=0.3, interpolation=interpolation)
    rows, cols = np.mgrid[0:61, 0:61] - 30
    outside = np.hypot(rows, cols) >= 18
    np.testing.assert_allclose(out[outside], noise[outside], atol=1e-6)
    assert np.abs(out - noise)[~outside].max() > 0.05


@pytest.mark.parametrize(
    ("angle", "moved_to"),
    [(360.0, (75, 50)), (-360.0, (25, 50))],
    ids=["a positive angle turns clockwise", "a negative angle turns counter-clockwise"],
)
def test_a_pixel_moves_around_the_centre_by_the_angle_left_at_its_distance(
    angle: float, moved_to: tuple[int, int]
) -> None:
    """Radius 0.5 of 100 px is 50 px; the twist falls off as (1 - distance / radius)^2, so a pixel 25 px right of the
    centre (50, 50) is turned by angle x (1 - 25 / 50)^2 = angle / 4 = 90 degrees: clockwise, onto the pixel 25 px
    below the centre, or counter-clockwise, onto the pixel 25 px above it."""
    out = registry.apply(NAME, impulse(101, 50, 75), angle=angle, radius=0.5, interpolation="nearest")[..., 0]
    assert np.argwhere(out == 1.0).tolist() == [list(moved_to)]
    assert out.sum() == 1.0


@pytest.mark.parametrize("angle", [200.0, -45.0, 720.0])
def test_a_swirl_keeps_every_pixels_distance_from_the_centre(angle: float) -> None:
    """A swirl only turns. On a picture whose every pixel has a value of its own (its index), with 'nearest', each
    pixel shows exactly one pixel of the original; that one is as far from the centre (50, 50) as the pixel itself, to
    within the half-pixel rounding of 'nearest' (at most 0.71 px)."""
    size = 101
    index = np.arange(size * size).reshape(size, size)
    out = registry.apply(NAME, grey(index / (size * size)), angle=angle, radius=0.5, interpolation="nearest")
    shown = np.rint(out[..., 0].astype(np.float64) * size * size).astype(int)  # the original index each pixel shows
    source_rows, source_cols = np.divmod(shown, size)
    rows, cols = np.mgrid[0:size, 0:size]
    np.testing.assert_allclose(np.hypot(source_rows - 50, source_cols - 50), np.hypot(rows - 50, cols - 50), atol=0.75)
    assert (shown != index).sum() > 1000  # and the picture really is twisted


@pytest.mark.parametrize("interpolation", ["bilinear", "bicubic"])
@pytest.mark.parametrize("angle", [90.0, -250.0])
def test_each_pixel_shows_the_picture_at_the_point_turned_about_the_centre_by_the_local_angle(
    interpolation: str, angle: float
) -> None:
    """A linear picture is read exactly between its pixels by bilinear and (away from the edge) bicubic interpolation,
    so a brute-force reference can say what every pixel must show: the ramp at the pixel's offset from the centre
    (30, 30) turned by angle x (1 - distance / 18)^2 degrees, clockwise (rows run down, columns right)."""
    rows, cols = np.mgrid[0:61, 0:61].astype(np.float64)
    ramp = (2 * rows + cols + 60) / 240.0  # linear in both directions, within 0..1
    out = registry.apply(NAME, grey(ramp), angle=angle, radius=0.3, interpolation=interpolation)[..., 0]
    expected = np.empty_like(ramp)
    for row in range(61):
        for col in range(61):
            dr, dc = row - 30.0, col - 30.0
            distance = math.hypot(dr, dc)
            turn = math.radians(angle) * (1 - distance / 18) ** 2 if distance < 18 else 0.0
            source_row = 30 + dr * math.cos(turn) - dc * math.sin(turn)
            source_col = 30 + dr * math.sin(turn) + dc * math.cos(turn)
            expected[row, col] = (2 * source_row + source_col + 60) / 240.0
    np.testing.assert_allclose(out, expected, atol=1e-5)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.0), ("mirror", 0.5), ("reflect", 0.45), ("wrap", 0.55)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_swirl_turns_it_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 rows whose value is row / 20, swirled about its top left pixel with radius 1 (20 px) and angle
    360: the pixel (0, 10) is 10 px from the centre, turned by 360 x (1 - 10 / 20)^2 = 90 degrees clockwise, so it
    shows the point 10 px above the picture, where row -10 is
    nearest: the first row, 0; mirror: row 10 (mirrored about row 0), 0.5; reflect: row 9 (mirrored about the edge of
    the first pixel, which repeats it), 0.45; wrap: row 11 (the picture repeated, 21 rows), 0.55."""
    rows = np.tile((np.arange(21) / 20.0)[:, None], (1, 21))
    out = registry.apply(NAME, grey(rows), angle=360.0, radius=1.0, x=0.0, y=0.0, edge=edge)
    assert out[0, 10, 0] == pytest.approx(expected, abs=1e-5)


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_constant_picture_stays_as_it_is(edge: str, interpolation: str) -> None:
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, angle=700.0, radius=2.0, x=0.2, y=0.9, edge=edge, interpolation=interpolation)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_nearest_only_moves_whole_pixels_where_bilinear_blends_them() -> None:
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    nearest = registry.apply(NAME, noise, angle=120.0, radius=0.8, interpolation="nearest")
    bilinear = registry.apply(NAME, noise, angle=120.0, radius=0.8, interpolation="bilinear")
    assert np.isin(nearest, noise).all()
    assert not np.isin(bilinear, noise).all()


def test_each_colour_channel_is_moved_the_same_way() -> None:
    green_dot = np.zeros((101, 101, 3), np.float32)
    green_dot[50, 75, 1] = 1.0
    out = registry.apply(NAME, green_dot, angle=360.0, radius=0.5, interpolation="nearest")
    assert out[75, 50].tolist() == [0.0, 1.0, 0.0]
    assert out[..., [0, 2]].max() == 0.0


def test_bicubic_overshoot_is_clipped_to_the_range_of_a_picture() -> None:
    edge = np.zeros((41, 41, 3), np.float32)
    edge[:, 20:] = 1.0
    out = registry.apply(NAME, edge, angle=150.0, radius=0.9, interpolation="bicubic")
    assert out.min() >= 0.0
    assert out.max() <= 1.0
