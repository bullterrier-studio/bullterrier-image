"""Behaviour of the `parallax` effect: the picture's layers shifted by their depth, as if the camera moved sideways."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "parallax"
HEIGHT, WIDTH = 96, 128  # the long side is 128 px: amount 0.0625 is 8 px per unit of depth from the focus
AMOUNT = 0.0625
MARGIN = 8  # px from each border: no shift below reaches farther, so inside it no pixel reads beyond the picture


def noise(height: int = HEIGHT, width: int = WIDTH) -> np.ndarray:
    """Every pixel unlike its neighbours: any shift shows."""
    return np.random.default_rng(1).random((height, width, 3), dtype=np.float32)


def flat(value: float, height: int = HEIGHT, width: int = WIDTH) -> np.ndarray:
    """A depth of one plane at `value`."""
    return np.full((height, width), value, np.float32)


def interior(picture: np.ndarray) -> np.ndarray:
    """`picture` without its MARGIN px border."""
    return picture[MARGIN:-MARGIN, MARGIN:-MARGIN]


@pytest.mark.parametrize(
    ("settings", "depth"),
    [
        pytest.param({"amount": 0.0, "focus": 0.5}, 1.0, id="amount 0 shifts nothing"),
        pytest.param({"amount": 0.1, "focus": 0.3}, 0.3, id="every depth at the focus stays put"),
        pytest.param({"amount": 0.1, "focus": 1.0, "angle": 45.0}, 1.0, id="focus 1 on the nearest plane"),
    ],
)
def test_a_picture_with_nothing_to_shift_is_returned_exactly(settings: dict[str, float], depth: float) -> None:
    picture = noise()
    out = registry.apply(NAME, picture, depth=flat(depth), **settings)
    np.testing.assert_array_equal(out, picture)


@pytest.mark.parametrize(
    ("height", "width", "depth", "focus", "angle", "down", "right"),
    [
        pytest.param(HEIGHT, WIDTH, 1.0, 0.5, 0.0, 0, 4, id="nearer than the focus, angle 0: right by 8 x 0.5"),
        pytest.param(HEIGHT, WIDTH, 1.0, 0.5, 90.0, -4, 0, id="angle 90: up, columns untouched"),
        pytest.param(HEIGHT, WIDTH, 1.0, 0.5, 180.0, 0, -4, id="angle 180: left, the opposite of 0"),
        pytest.param(HEIGHT, WIDTH, 1.0, 0.5, 270.0, 4, 0, id="angle 270: down"),
        pytest.param(HEIGHT, WIDTH, 0.75, 0.5, 0.0, 0, 2, id="a quarter from the focus: 8 x 0.25"),
        pytest.param(HEIGHT, WIDTH, 0.0, 0.5, 0.0, 0, -4, id="farther than the focus: against the angle"),
        pytest.param(HEIGHT, WIDTH, 0.25, 1.0, 90.0, 6, 0, id="0.75 behind the focus, angle 90: down by 6"),
        pytest.param(WIDTH, 64, 1.0, 0.5, 0.0, 0, 4, id="portrait: the long side is the height"),
    ],
)
@pytest.mark.parametrize("interpolation", ["bilinear", "bicubic"])
def test_a_single_plane_is_translated_by_its_distance_from_the_focus(
    height: int, width: int, depth: float, focus: float, angle: float, down: int, right: int, interpolation: str
) -> None:
    """The shift is amount x long side x (depth - focus) px along `angle` (0 = right, 90 = up); the expected picture
    is the input rolled by that many whole px, compared away from the borders, where the edge rule decides."""
    picture = noise(height, width)
    out = registry.apply(
        NAME,
        picture,
        depth=flat(depth, height, width),
        amount=AMOUNT,
        focus=focus,
        angle=angle,
        interpolation=interpolation,
    )
    expected = np.roll(picture, (down, right), axis=(0, 1))
    np.testing.assert_allclose(interior(out), interior(expected), atol=1e-5)


def test_layers_either_side_of_the_focus_move_opposite_ways() -> None:
    """Left half nearest (1), right half farthest (0), focus 0.5: the near half moves right by 4 px and covers the far
    half's first 4 columns; the far half moves left by 4 px. Columns 4..67 show the near half, 68.. the far half."""
    picture = noise()
    depth = flat(0.0)
    depth[:, : WIDTH // 2] = 1.0
    out = registry.apply(NAME, picture, depth=depth, amount=AMOUNT, focus=0.5)
    np.testing.assert_allclose(out[:, 4:68], picture[:, 0:64], atol=1e-5)
    np.testing.assert_allclose(out[:, 68:124], picture[:, 72:128], atol=1e-5)


@pytest.mark.parametrize(
    ("angle", "down", "right"),
    [pytest.param(0.0, 0, 8, id="angle 0: right by 8"), pytest.param(90.0, -8, 0, id="angle 90: up by 8")],
)
def test_a_near_object_moves_whole_over_a_still_background(angle: float, down: int, right: int) -> None:
    """A bright 16 px square at depth 1 on a dark background at the focus, 0: the square moves 8 px and lands whole,
    every one of its pixels and no other bright. Reading each output pixel's own depth would keep only the half of
    the square that lands on its old place."""
    picture = np.full((HEIGHT, WIDTH, 3), 0.1, np.float32)
    square = np.zeros((HEIGHT, WIDTH), bool)
    square[40:56, 56:72] = True
    picture[square] = 0.9
    out = registry.apply(NAME, picture, depth=square.astype(np.float32), amount=AMOUNT, focus=0.0, angle=angle)
    bright = out[..., 0] > 0.5
    np.testing.assert_array_equal(bright, np.roll(square, (down, right), axis=(0, 1)))


def test_the_shift_never_leaves_the_range_of_the_picture() -> None:
    """Bilinear reading blends neighbours, so no value lies outside the picture's own range."""
    picture = noise() * 0.5 + 0.3  # 0.3..0.8: away from 0 and 1, so a wider range would show
    ramp = np.tile(np.linspace(1, 0, WIDTH, dtype=np.float32), (HEIGHT, 1))
    out = registry.apply(NAME, picture, depth=ramp, amount=0.03, focus=0.4, angle=30.0)
    assert out.min() >= picture.min()
    assert out.max() <= picture.max()


def test_without_a_depth_the_picture_is_returned_unchanged_as_a_copy() -> None:
    picture = noise()
    out = registry.apply(NAME, picture, amount=0.1)
    np.testing.assert_array_equal(out, picture)
    assert out is not picture
