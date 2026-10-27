"""Behaviour of the `rgb_split` effect: red pushed one way along an angle, blue (and for red-cyan green) the other."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "rgb_split"
FLOAT32_TOLERANCE = 2 * np.finfo(np.float32).eps  # two float32 epsilons: what a differently ordered sum may differ by
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
        pytest.param(90, 2, (7, 10), (3, 10), id="angle 90: red down, blue up"),
        pytest.param(180, 4, (5, 6), (5, 14), id="angle 180: red to the left, blue to the right"),
        pytest.param(270, 1, (4, 10), (6, 10), id="angle 270: red up, blue down"),
        pytest.param(45, 0, (5, 10), (5, 10), id="amount 0: nobody moves"),
    ],
)
def test_red_and_blue_separate_by_exactly_the_amount_in_opposite_directions_and_green_stays(
    angle, amount, red, blue
) -> None:
    """Angles count clockwise from pointing right on screen, as CSS `rotate`; rows count down, so down is more rows."""
    out = registry.apply(NAME, impulse(), angle=angle, amount=amount, edge="nearest")
    assert lit(out, 0) == pytest.approx({red: 1.0})
    assert lit(out, 1) == pytest.approx({(ROW, COLUMN): 1.0})
    assert lit(out, 2) == pytest.approx({blue: 1.0})


def test_an_offset_that_ends_between_pixels_is_shared_by_the_pixels_it_lies_between() -> None:
    """Angle 330, amount 8: red moves 8 cos 30 = 4 sqrt 3 = 6.928 px right and 8 sin 30 = 4 px up, blue the same
    distance the other way. Position 10 + 6.928 = 16.928 puts 0.072 of the pixel on column 16 and 0.928 on column 17,
    and 10 - 6.928 = 3.072 puts 0.928 on column 3 and 0.072 on column 4."""
    right = 4 * np.sqrt(3) - 6
    out = registry.apply(NAME, impulse(), angle=330, amount=8, edge="nearest")
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


@pytest.mark.parametrize(("angle", "sign"), [(0, 1), (180, -1)], ids=["angle 0: red right", "angle 180: red left"])
def test_with_wrap_a_whole_number_of_pixels_rolls_red_and_blue_round_the_picture(angle, sign, photo) -> None:
    """The glitch look: red rolled `amount` columns one way, blue the other, green untouched."""
    out = registry.apply(NAME, photo, angle=angle, amount=7, edge="wrap")
    np.testing.assert_allclose(out[..., 0], np.roll(photo[..., 0], sign * 7, axis=1), atol=FLOAT32_TOLERANCE)
    np.testing.assert_array_equal(out[..., 1], photo[..., 1])
    np.testing.assert_allclose(out[..., 2], np.roll(photo[..., 2], -sign * 7, axis=1), atol=FLOAT32_TOLERANCE)


@pytest.mark.parametrize("edge", ["wrap", "nearest", "reflect", "mirror"])
def test_a_flat_picture_stays_flat(edge: str) -> None:
    out = registry.apply(NAME, np.full((9, 11, 3), 0.3, np.float32), angle=77, amount=6.5, edge=edge)
    np.testing.assert_allclose(out, 0.3, atol=1e-6)


# red to the left, green and blue to the right
RED_CYAN_LEFT_RIGHT = {"pair": "red_cyan", "angle": 180, "edge": "nearest"}


@pytest.mark.parametrize(
    ("amount", "red", "cyan"),
    [(3, 7, 13), (1, 9, 11), (0, 10, 10)],
    ids=["3 px: red 3 left, cyan 3 right", "1 px", "0 px: both where the pixel is"],
)
def test_red_cyan_moves_green_and_blue_together_opposite_to_red(amount: int, red: int, cyan: int) -> None:
    out = registry.apply(NAME, impulse(), amount=amount, **RED_CYAN_LEFT_RIGHT)
    assert lit(out, 0) == pytest.approx({(ROW, red): 1.0})
    assert lit(out, 1) == pytest.approx({(ROW, cyan): 1.0})
    assert lit(out, 2) == pytest.approx({(ROW, cyan): 1.0})


def test_red_cyan_shares_an_offset_between_the_pixels_it_lies_between() -> None:
    """2.25 px: the white pixel at column 10 lands at 7.75 (red) and 12.25 (cyan), so a quarter of it on column 7 and
    three quarters on 8, and three quarters on 12 and a quarter on 13."""
    out = registry.apply(NAME, impulse(), amount=2.25, **RED_CYAN_LEFT_RIGHT)
    assert lit(out, 0) == pytest.approx({(ROW, 7): 0.25, (ROW, 8): 0.75})
    assert lit(out, 1) == pytest.approx({(ROW, 12): 0.75, (ROW, 13): 0.25})


@pytest.mark.parametrize("angle", [0, 33, 217])
def test_the_pair_changes_only_green(angle: float, photo: np.ndarray) -> None:
    """Red and blue are the same for both pairs; green stays with 'red_blue' and moves with 'red_cyan'."""
    red_blue = registry.apply(NAME, photo, pair="red_blue", angle=angle, amount=7.5)
    red_cyan = registry.apply(NAME, photo, pair="red_cyan", angle=angle, amount=7.5)
    np.testing.assert_array_equal(red_cyan[..., 0], red_blue[..., 0])
    np.testing.assert_array_equal(red_cyan[..., 2], red_blue[..., 2])
    np.testing.assert_array_equal(red_blue[..., 1], photo[..., 1])
    assert not np.array_equal(red_cyan[..., 1], photo[..., 1])


@pytest.mark.parametrize("angle", [0, 33, 217])
def test_red_cyan_moves_green_exactly_as_blue(angle: float, photo: np.ndarray) -> None:
    """On a grey picture (green equal to blue) the two end up equal."""
    grey = np.repeat(photo[..., 1:2], 3, axis=2)
    out = registry.apply(NAME, grey, pair="red_cyan", angle=angle, amount=7.5)
    np.testing.assert_array_equal(out[..., 1], out[..., 2])


@pytest.mark.parametrize("pair", ["red_blue", "red_cyan"])
def test_with_no_amount_the_picture_is_unchanged(pair: str, photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, pair=pair, amount=0), photo)


def test_after_grade_to_grey_red_cyan_is_the_brightness_seen_from_two_sides(photo: np.ndarray) -> None:
    """The anaglyph recipe: grey the picture, then split red against cyan. Oracle by hand: Rec. 709 luminance, and
    for 4 px at angle 180 red column j reads the brightness at column j + 4 and green and blue at column j - 4 (the edge
    column held)."""
    luma = 0.2126 * photo[..., 0] + 0.7152 * photo[..., 1] + 0.0722 * photo[..., 2]
    columns = np.arange(photo.shape[1])
    grey = registry.apply("grade", photo, saturation=0)
    out = registry.apply(NAME, grey, amount=4, **RED_CYAN_LEFT_RIGHT)
    left = luma[:, np.clip(columns + 4, 0, photo.shape[1] - 1)]
    right = luma[:, np.clip(columns - 4, 0, photo.shape[1] - 1)]
    np.testing.assert_allclose(out[..., 0], left, atol=FLOAT32_TOLERANCE)
    np.testing.assert_allclose(out[..., 1], right, atol=FLOAT32_TOLERANCE)
    np.testing.assert_allclose(out[..., 2], right, atol=FLOAT32_TOLERANCE)


@pytest.mark.parametrize(
    ("edge", "red_at_last", "cyan_at_first"),
    [("nearest", 1.0, 0.0), ("wrap", 0.1, 0.9), ("mirror", 0.8, 0.2)],
    ids=["nearest repeats the edge column", "wrap brings back the far side", "mirror reflects about the edge column"],
)
def test_edge_says_what_a_red_cyan_view_shows_beyond_the_picture(
    edge: str, red_at_last: float, cyan_at_first: float
) -> None:
    """A grey ramp of 11 columns, value column / 10, moved by 2: red at the last column reads column 12, beyond the
    edge (nearest: column 10; wrap: column 1; mirror: column 8) and cyan at column 0 reads column -2 (nearest: column
    0; wrap: column 9; mirror: column 2)."""
    ramp = np.repeat(np.tile(np.arange(11) / 10.0, (3, 1))[..., None], 3, axis=2).astype(np.float32)
    out = registry.apply(NAME, ramp, pair="red_cyan", angle=180, amount=2, edge=edge)
    assert out[1, 10, 0] == pytest.approx(red_at_last, abs=1e-6)
    assert out[1, 0, 1] == pytest.approx(cyan_at_first, abs=1e-6)
