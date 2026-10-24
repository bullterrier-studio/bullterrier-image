"""Behaviour of the `rgb_split` effect: the red and blue channels pushed apart along an angle, green left in place."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "rgb_split"
ROW, COLUMN = 5, 10  # where the white pixel sits on the black 12 x 24 picture of the tests below


def impulse() -> np.ndarray:
    """A black 12 x 24 picture with one white pixel, at (ROW, COLUMN)."""
    rgb = np.zeros((12, 24, 3), np.float32)
    rgb[ROW, COLUMN] = 1.0
    return rgb


def lit(rgb: np.ndarray, channel: int) -> dict[tuple[int, int], float]:
    """The pixels of one channel that are visibly not black (above 1e-6: a sine of 180 degrees is not quite 0), by (row,
    column), with their value."""
    plane = rgb[..., channel]
    return {(int(r), int(c)): float(plane[r, c]) for r, c in zip(*np.nonzero(plane > 1e-6), strict=True)}


@pytest.mark.parametrize(
    ("angle", "amount", "red", "blue"),
    [
        pytest.param(0, 3, (5, 13), (5, 7), id="angle 0: red to the right, blue to the left"),
        pytest.param(90, 2, (3, 10), (7, 10), id="angle 90: red up, blue down"),
        pytest.param(180, 4, (5, 6), (5, 14), id="angle 180: red to the left, blue to the right"),
        pytest.param(270, 1, (6, 10), (4, 10), id="angle 270: red down, blue up"),
        pytest.param(45, 0, (5, 10), (5, 10), id="amount 0: nobody moves"),
    ],
)
def test_red_and_blue_separate_by_exactly_the_amount_in_opposite_directions_and_green_stays(
    angle, amount, red, blue
) -> None:
    """Angles count counter-clockwise from pointing right, as on a protractor; rows count down, so up is fewer rows."""
    out = registry.apply(NAME, impulse(), angle=angle, amount=amount, edge="nearest")
    assert lit(out, 0) == pytest.approx({red: 1.0})
    assert lit(out, 1) == pytest.approx({(ROW, COLUMN): 1.0})
    assert lit(out, 2) == pytest.approx({blue: 1.0})


def test_an_offset_that_ends_between_pixels_is_shared_by_the_pixels_it_lies_between() -> None:
    """Angle 30, amount 8: red moves 8 cos 30 = 4 sqrt 3 = 6.928 px right and 8 sin 30 = 4 px up, blue the same
    distance the other way. Position 10 + 6.928 = 16.928 puts 0.072 of the pixel on column 16 and 0.928 on column 17,
    and 10 - 6.928 = 3.072 puts 0.928 on column 3 and 0.072 on column 4."""
    right = 4 * np.sqrt(3) - 6
    out = registry.apply(NAME, impulse(), angle=30, amount=8, edge="nearest")
    assert lit(out, 0) == pytest.approx({(1, 16): 1 - right, (1, 17): right})
    assert lit(out, 2) == pytest.approx({(9, 3): right, (9, 4): 1 - right})


@pytest.mark.parametrize("angle", [0, 33, 90, 217])
def test_the_green_channel_is_never_touched(angle: float, photo) -> None:
    out = registry.apply(NAME, photo, angle=angle, amount=7.5)
    np.testing.assert_array_equal(out[..., 1], photo[..., 1])


def test_on_a_picture_red_and_blue_are_the_picture_shifted_with_the_edge_pixel_held(photo) -> None:
    """Angle 0, amount 5 px: red column j shows the picture's column j - 5 (the first column held where j < 5), blue
    column j shows column j + 5 (the last held), read here with explicit index arrays."""
    width = photo.shape[1]
    columns = np.arange(width)
    out = registry.apply(NAME, photo, angle=0, amount=5, edge="nearest")
    np.testing.assert_allclose(out[..., 0], photo[:, np.clip(columns - 5, 0, width - 1), 0], atol=1e-6)
    np.testing.assert_allclose(out[..., 2], photo[:, np.clip(columns + 5, 0, width - 1), 2], atol=1e-6)


@pytest.mark.parametrize(
    ("edge", "red", "blue"),
    [
        pytest.param(
            "wrap",
            {(5, 1): 1.0, (7, 8): 1.0},
            {(5, 15): 1.0, (7, 22): 1.0},
            id="wrap: what leaves comes back on the other side",
        ),
        pytest.param("nearest", {(7, 8): 1.0}, {(5, 15): 1.0}, id="nearest: what leaves is gone"),
    ],
)
def test_edge_decides_what_a_channel_that_leaves_the_picture_brings_back(edge: str, red, blue) -> None:
    """Two pixels of a 24-column picture, at column 20 of row 5 and column 3 of row 7, moved 5 px (angle 0): red's
    first goes to column 25, which wraps to 1, its second to 8; blue's first goes to 15, its second to -2, which wraps
    to 22."""
    rgb = np.zeros((12, 24, 3), np.float32)
    rgb[5, 20] = 1.0
    rgb[7, 3] = 1.0
    out = registry.apply(NAME, rgb, angle=0, amount=5, edge=edge)
    assert lit(out, 0) == pytest.approx(red)
    assert lit(out, 1) == pytest.approx({(5, 20): 1.0, (7, 3): 1.0})
    assert lit(out, 2) == pytest.approx(blue)


@pytest.mark.parametrize("edge", ["wrap", "nearest", "reflect", "mirror"])
def test_a_flat_picture_stays_flat(edge: str) -> None:
    out = registry.apply(NAME, np.full((9, 11, 3), 0.3, np.float32), angle=77, amount=6.5, edge=edge)
    np.testing.assert_allclose(out, 0.3, atol=1e-6)
