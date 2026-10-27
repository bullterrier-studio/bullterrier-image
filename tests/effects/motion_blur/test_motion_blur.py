"""Behaviour of the `motion_blur` effect: the picture averaged along a straight line of `length` px at `angle`."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "motion_blur"


def impulse(size: int = 61) -> np.ndarray:
    """A black picture with one white pixel in the middle."""
    dark = np.zeros((size, size, 3), np.float32)
    dark[size // 2, size // 2] = 1.0
    return dark


def row_average(values: np.ndarray, length: float) -> np.ndarray:
    """The brute-force definition of a horizontal line blur of one row: each output pixel is the average of the picture
    (every pixel a constant 1 x 1 square) over the interval of `length` px centred on it, with the picture mirrored
    beyond its ends (edge pixel repeated). Overlaps are worked out one pixel at a time."""
    reach = int(np.ceil(length / 2)) + 1
    padded = np.concatenate([values[:reach][::-1], values, values[-reach:][::-1]])  # reflect
    out = np.zeros(len(values))
    for x in range(len(values)):
        total = 0.0
        for i in range(-reach, reach + 1):
            left = max(i - 0.5, -length / 2)
            right = min(i + 0.5, length / 2)
            total += max(0.0, right - left) * padded[reach + x + i]
        out[x] = total / length
    return out


LENGTHS = [1.0, 4.0, 5.0, 7.5, 12.0]


@pytest.mark.parametrize("length", LENGTHS)
def test_angle_0_averages_each_row_over_the_length(length: float) -> None:
    rng = np.random.default_rng(3)
    stripes = np.repeat(rng.random((9, 40, 1), dtype=np.float32), 3, axis=2)
    out = registry.apply(NAME, stripes, angle=0.0, length=length, edge="reflect")
    for y in range(9):
        np.testing.assert_allclose(out[y, :, 0], row_average(stripes[y, :, 0], length), atol=2e-6)


@pytest.mark.parametrize("length", LENGTHS)
def test_angle_90_averages_each_column_over_the_length(length: float) -> None:
    rng = np.random.default_rng(4)
    stripes = np.repeat(rng.random((40, 9, 1), dtype=np.float32), 3, axis=2)
    out = registry.apply(NAME, stripes, angle=90.0, length=length, edge="reflect")
    for x in range(9):
        np.testing.assert_allclose(out[:, x, 0], row_average(stripes[:, x, 0], length), atol=2e-6)


@pytest.mark.parametrize(
    ("angle", "length", "weights"),
    [
        (0.0, 5.0, {(0, -2): 0.2, (0, -1): 0.2, (0, 0): 0.2, (0, 1): 0.2, (0, 2): 0.2}),
        (0.0, 4.0, {(0, -2): 0.125, (0, -1): 0.25, (0, 0): 0.25, (0, 1): 0.25, (0, 2): 0.125}),
        (90.0, 3.0, {(-1, 0): 1 / 3, (0, 0): 1 / 3, (1, 0): 1 / 3}),
        (180.0, 3.0, {(0, -1): 1 / 3, (0, 0): 1 / 3, (0, 1): 1 / 3}),
        (
            45.0,
            2 * np.sqrt(2),
            {(-1, -1): 0.25, (0, 0): 0.5, (1, 1): 0.25},
        ),  # clockwise from right: down and to the right
        (135.0, 2 * np.sqrt(2), {(1, -1): 0.25, (0, 0): 0.5, (-1, 1): 0.25}),
        (0.0, 0.0, {(0, 0): 1.0}),
        (30.0, 0.5, {(0, 0): 1.0}),
    ],
    ids=[
        "5 px",
        "4 px: half-weight ends",
        "vertical",
        "180 degrees",
        "45 degrees",
        "135 degrees",
        "0 px",
        "under a pixel",
    ],
)
def test_an_impulse_becomes_the_line_worked_out_by_hand(angle: float, length: float, weights: dict) -> None:
    """The picture's response to one white pixel is the line itself: each pixel weighs the length of the line inside
    it (a 1 x 1 square), the weights summing to 1. Rows are counted downwards, columns to the right."""
    out = registry.apply(NAME, impulse(), angle=angle, length=length)[..., 0]
    expected = np.zeros_like(out)
    for (dy, dx), weight in weights.items():
        expected[30 + dy, 30 + dx] = weight
    np.testing.assert_allclose(out, expected, atol=2e-6)


@pytest.mark.parametrize("angle", [10.0, 33.0, 77.0, 120.0, 250.0])
def test_the_line_has_the_length_and_direction_asked_for(angle: float) -> None:
    """The blurred impulse is spread like a uniform line of 20 px: variance length^2 / 12 along it (plus about 1/12 for
    the pixel's own size), and next to nothing across it, centred on the pixel, whatever the angle."""
    length = 20.0
    out = registry.apply(NAME, impulse(81), angle=angle, length=length)[..., 0].astype(np.float64)
    rows, cols = np.mgrid[0:81, 0:81] - 40
    right, down = cols, rows
    along = right * np.cos(np.radians(angle)) + down * np.sin(np.radians(angle))
    across = -right * np.sin(np.radians(angle)) + down * np.cos(np.radians(angle))
    assert out.sum() == pytest.approx(1.0, abs=1e-5)
    assert (out * along).sum() == pytest.approx(0.0, abs=0.02)
    assert (out * along**2).sum() == pytest.approx(length**2 / 12, abs=0.5)
    assert (out * across**2).sum() < 0.2


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
def test_a_constant_picture_stays_as_it_is(edge: str) -> None:
    grey = np.full((21, 34, 3), 0.4, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, grey, angle=37.0, length=9.0, edge=edge), 0.4, atol=1e-6)


def test_the_blur_keeps_the_mean_when_the_picture_wraps_around(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, angle=25.0, length=11.0, edge="wrap")
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=1e-5)
    assert out.std() < photo.std()


def test_length_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, length=0.0), photo)


@pytest.mark.parametrize(("angle", "unchanged_along"), [(0.0, "rows"), (90.0, "columns")])
def test_detail_along_the_line_is_smeared_and_detail_across_it_is_kept(angle: float, unchanged_along: str) -> None:
    """Horizontal blur leaves a picture that only varies down the rows exactly as it is; vertical blur does for one
    that only varies along the columns."""
    ramp = np.linspace(0.1, 0.9, 32, dtype=np.float32)
    if unchanged_along == "rows":
        banded = np.tile(ramp[:, None, None], (1, 24, 3))
    else:
        banded = np.tile(ramp[None, :, None], (24, 1, 3))
    np.testing.assert_allclose(registry.apply(NAME, banded, angle=angle, length=9.0), banded, atol=2e-6)
    across = registry.apply(NAME, banded, angle=angle + 90.0, length=9.0)
    assert np.abs(across - banded).max() > 0.01


def test_a_longer_line_blurs_more() -> None:
    rng = np.random.default_rng(0)
    noise = rng.random((32, 32, 3), dtype=np.float32)
    spread = [float(registry.apply(NAME, noise, angle=20.0, length=n).std()) for n in (0.0, 4.0, 12.0, 30.0)]
    assert spread == sorted(spread, reverse=True)
    assert spread[0] > spread[-1] * 1.5


def test_each_colour_channel_is_blurred_on_its_own() -> None:
    red_dot = impulse()
    red_dot[..., 1:] = 0.0
    out = registry.apply(NAME, red_dot, length=9.0)
    assert out[30, 33, 0] > 0
    assert out[..., 1:].max() == 0.0


@pytest.mark.parametrize(
    ("edge", "bright_column", "expected"),
    [
        ("reflect", 0, 0.4),  # the edge pixel is repeated once beyond the edge: 2 of the 5 samples
        ("mirror", 0, 0.2),  # the pixel is the mirror's axis: it is seen once
        ("nearest", 0, 0.6),  # the edge pixel is repeated throughout: 3 of the 5 samples
        ("wrap", 0, 0.2),  # the other edge is 20 px away: the pixel is seen once
        ("reflect", 2, 0.2),  # mirrored beyond the edge, the third pixel is never seen again
        ("mirror", 2, 0.4),  # the third pixel is seen twice, as the sample 2 px beyond the edge
        ("nearest", 2, 0.2),
        ("wrap", 2, 0.2),
    ],
)
def test_edge_sets_what_the_blur_sees_beyond_the_edge(edge: str, bright_column: int, expected: float) -> None:
    """One bright pixel in a dark row, blurred along the row by 5 px; the first pixel of the row is read at offsets
    -2 to 2, whose beyond-the-edge samples each edge rule fills differently."""
    dark = np.zeros((9, 21, 3), np.float32)
    dark[4, bright_column] = 1.0
    out = registry.apply(NAME, dark, angle=0.0, length=5.0, edge=edge)
    assert out[4, 0, 0] == pytest.approx(expected, abs=1e-6)


def test_the_result_never_leaves_the_range_of_the_picture(photo: np.ndarray) -> None:
    """Averaging cannot make a value brighter than the brightest or darker than the darkest: not even by rounding."""
    out = registry.apply(NAME, photo, angle=33.0, length=7.0)
    assert out.min() >= photo.min()
    assert out.max() <= photo.max()
    flat = np.full((20, 20, 3), 0.7, np.float32)
    assert (registry.apply(NAME, flat, angle=33.0, length=7.0) == np.float32(0.7)).all()
