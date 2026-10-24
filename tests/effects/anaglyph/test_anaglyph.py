"""Behaviour of the `anaglyph` effect: a red and a cyan view of the brightness, moved apart sideways."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "anaglyph"
ROW, COLUMN = 5, 10  # where the white pixel sits on the black 12 x 24 picture below


def impulse() -> np.ndarray:
    """A black 12 x 24 picture with one white pixel, at (ROW, COLUMN)."""
    rgb = np.zeros((12, 24, 3), np.float32)
    rgb[ROW, COLUMN] = 1.0
    return rgb


def lit(out: np.ndarray, channel: int) -> dict[tuple[int, int], float]:
    plane = out[..., channel]
    return {(int(r), int(c)): float(plane[r, c]) for r, c in zip(*np.nonzero(plane > 1e-6), strict=True)}


def test_with_no_shift_all_three_channels_are_the_brightness(photo: np.ndarray) -> None:
    """Brightness by hand from the Rec. 709 weights."""
    luma = 0.2126 * photo[..., 0] + 0.7152 * photo[..., 1] + 0.0722 * photo[..., 2]
    out = registry.apply(NAME, photo, shift=0)
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], luma, atol=1e-6)


@pytest.mark.parametrize(
    ("shift", "red", "cyan"),
    [(3, 7, 13), (1, 9, 11), (0, 10, 10)],
    ids=["3 px: red 3 left, cyan 3 right", "1 px", "0 px: both where the pixel is"],
)
def test_red_is_the_brightness_moved_left_and_green_and_blue_the_brightness_moved_right(
    shift: int, red: int, cyan: int
) -> None:
    out = registry.apply(NAME, impulse(), shift=shift, edge="nearest")
    assert lit(out, 0) == pytest.approx({(ROW, red): 1.0})
    assert lit(out, 1) == pytest.approx({(ROW, cyan): 1.0})
    assert lit(out, 2) == pytest.approx({(ROW, cyan): 1.0})


def test_a_shift_between_pixels_is_shared_by_the_two_it_lies_between() -> None:
    """2.25 px: the white pixel at column 10 lands at 7.75 (red) and 12.25 (cyan), so a quarter of it on column 7 and
    three quarters on 8, and three quarters on 12 and a quarter on 13."""
    out = registry.apply(NAME, impulse(), shift=2.25, edge="nearest")
    assert lit(out, 0) == pytest.approx({(ROW, 7): 0.25, (ROW, 8): 0.75})
    assert lit(out, 1) == pytest.approx({(ROW, 12): 0.75, (ROW, 13): 0.25})


@pytest.mark.parametrize(
    ("edge", "red_at_last", "cyan_at_first"),
    [("nearest", 1.0, 0.0), ("wrap", 0.1, 0.9), ("mirror", 0.8, 0.2)],
    ids=["nearest repeats the edge column", "wrap brings back the far side", "mirror reflects about the edge column"],
)
def test_edge_says_what_the_moved_view_shows_beyond_the_picture(
    edge: str, red_at_last: float, cyan_at_first: float
) -> None:
    """A grey ramp of 11 columns, value column / 10, shifted by 2: red at the last column reads column 12, beyond the
    edge (nearest: column 10; wrap: column 1; mirror: column 8) and cyan at column 0 reads column -2 (nearest: column
    0; wrap: column 9; mirror: column 2)."""
    ramp = np.repeat(np.tile(np.arange(11) / 10.0, (3, 1))[..., None], 3, axis=2).astype(np.float32)
    out = registry.apply(NAME, ramp, shift=2, edge=edge)
    assert out[1, 10, 0] == pytest.approx(red_at_last, abs=1e-6)
    assert out[1, 0, 1] == pytest.approx(cyan_at_first, abs=1e-6)
