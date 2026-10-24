"""Tests of `ops/radial.py`: a picture read again and again, zoomed or spun about a centre, and added up."""

from collections.abc import Sequence
from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.radial import accumulate

CENTRE = (4.0, 6.0)  # (row, column) of the middle pixel of a 9 x 13 picture


def dot(row: int, col: int, channel: int = 0, value: float = 1.0) -> np.ndarray:
    """A black 9 x 13 picture with one lit pixel in one channel."""
    pic = np.zeros((9, 13, 3), np.float32)
    pic[row, col, channel] = value
    return pic


def lit(out: np.ndarray, channel: int = 0) -> list[tuple[int, int]]:
    """(row, column) of every pixel of `channel` above 1e-6, in reading order."""
    return [(int(r), int(c)) for r, c in zip(*np.nonzero(out[..., channel] > 1e-6), strict=True)]


def accumulate_with(
    image: np.ndarray,
    kind: Literal["zoom", "spin"],
    offsets: Sequence[float],
    weights: Sequence[float],
    *,
    interpolation: Literal["nearest", "bilinear"] = "nearest",
) -> np.ndarray:
    """`accumulate` about the middle pixel, reading beyond the edge as the nearest pixel."""
    return accumulate(
        image, centre=CENTRE, kind=kind, offsets=np.asarray(offsets), weights=weights,
        interpolation=interpolation, edge="nearest",
    )  # fmt: skip


def test_zooming_by_less_than_one_moves_the_picture_outward_from_the_centre() -> None:
    """Each pixel reads the picture at centre + scale x (its offset from it), with scale 1 + offset = 0.5: a dot 2 px
    right of the centre is read by the pixel 4 px right of it; the pixels beside it read half-way between two
    pixels, which 'nearest' rounds, so the dot is enlarged to a patch around that pixel."""
    out = accumulate_with(dot(4, 8), "zoom", [-0.5], [1.0])
    assert out[4, 10, 0] == 1.0
    assert out[4, 8, 0] == 0.0
    rows, cols = zip(*lit(out), strict=True)
    assert set(rows) <= {3, 4, 5}
    assert set(cols) <= {9, 10, 11}


def test_zooming_by_more_than_one_moves_the_picture_toward_the_centre() -> None:
    out = accumulate_with(dot(4, 10), "zoom", [1.0], [1.0])  # scale 2: the dot 4 px out is read by the pixel 2 px out
    assert lit(out) == [(4, 8)]


@pytest.mark.parametrize(
    ("dot_at", "lit_at"),
    [((4, 8), (2, 6)), ((4, 4), (6, 6)), ((2, 6), (4, 4)), ((6, 6), (4, 8))],
    ids=["right goes up", "left goes down", "up goes left", "down goes right"],
)
def test_spinning_by_a_quarter_turn_carries_the_picture_counterclockwise_about_the_centre(
    dot_at: tuple[int, int], lit_at: tuple[int, int]
) -> None:
    """The angle is in radians. A pixel reads the picture at the centre plus its offset turned by that angle, so with
    +90 degrees the pixel 2 px above the centre reads the dot 2 px to the right of it: the right goes up."""
    out = accumulate_with(dot(*dot_at), "spin", [np.pi / 2], [1.0])
    assert lit(out) == [lit_at]


@pytest.mark.parametrize(
    ("kind", "offsets"), [("zoom", [-0.7, 0.3, 1.5]), ("spin", [-1.0, 0.4, 2.5])], ids=["zoom", "spin"]
)
def test_the_centre_pixel_reads_itself_whatever_the_zoom_or_spin(
    kind: Literal["zoom", "spin"], offsets: list[float]
) -> None:
    out = accumulate_with(dot(4, 6), kind, offsets, [1.0, 1.0, 1.0])
    assert out[4, 6, 0] == 3.0


def test_the_result_is_the_weighted_sum_of_every_zoomed_picture() -> None:
    """On a ramp whose value is the column, bilinear reading at scale s gives centre column + s x (column - centre
    column) exactly; the weights then add up as plain numbers, worked out by hand."""
    ramp = np.tile(np.arange(13, dtype=np.float32)[None, :, None], (9, 1, 3))
    out = accumulate_with(ramp, "zoom", [-0.5, 0.0, 0.5], [0.25, 2.0, 1.0], interpolation="bilinear")
    column = np.arange(13)
    expected = 0.25 * (6 + 0.5 * (column - 6)) + 2.0 * column + 1.0 * (6 + 1.5 * (column - 6))
    np.testing.assert_allclose(out[4, 3:10, 0], expected[3:10], atol=1e-4)  # where every read stays in the picture


def test_a_zero_weight_drops_its_reading_and_an_empty_sum_is_black() -> None:
    out = accumulate_with(dot(4, 8), "zoom", [-0.5, 0.0], [0.0, 3.0])
    assert lit(out) == [(4, 8)]
    assert out[4, 8, 0] == pytest.approx(3.0)
    assert accumulate_with(dot(4, 8), "zoom", [], []).max() == 0.0


def test_each_colour_channel_is_read_on_its_own() -> None:
    out = accumulate_with(dot(4, 8, channel=1, value=0.5), "zoom", [-0.5], [2.0])
    assert out[4, 10, 1] == pytest.approx(1.0)
    assert out[..., [0, 2]].max() == 0.0


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.0), ("wrap", 1 / 12), ("reflect", 11 / 12), ("mirror", 1.0)],
    ids=["nearest repeats the first column", "wrap brings in the far side", "reflect", "mirror"],
)
def test_what_lies_beyond_the_edge_follows_the_edge_rule(
    edge: Literal["reflect", "mirror", "nearest", "wrap"], expected: float
) -> None:
    """Scale 3 about the centre: a pixel in column 0 reads column 6 + 3 x (0 - 6) = -12 of a ramp col / 12 that is 13
    columns wide. 'nearest' repeats the first column (0); 'wrap' reads column -12 + 13 = 1; 'reflect'
    (d c b a | a b c d) puts column 11 at -12; 'mirror' (d c b | a b c d) puts column 12 there."""
    ramp = np.tile(np.arange(13, dtype=np.float32)[None, :, None], (9, 1, 3)) / 12
    out = accumulate(
        ramp, centre=CENTRE, kind="zoom", offsets=np.array([2.0]), weights=[1.0], interpolation="nearest", edge=edge
    )
    assert out[4, 0, 0] == pytest.approx(expected, abs=1e-6)


def test_bicubic_reads_a_cubic_exactly_where_bilinear_and_nearest_do_not() -> None:
    """A picture (column / 60)^3 zoomed by 0.5 about column 30 is read at 30 + (column - 30) / 2: the cubic spline is
    exact on a cubic (away from the edge, whose fit is not), the straight lines of bilinear reading are not."""
    column = np.arange(61.0)
    cubic = np.tile(((column / 60) ** 3).astype(np.float32)[None, :, None], (3, 1, 3))
    reads = 30 + (column[20:41] - 30) / 2
    errors = {}
    for choice in ("nearest", "bilinear", "bicubic"):
        out = accumulate(
            cubic, centre=(1.0, 30.0), kind="zoom", offsets=np.array([-0.5]), weights=[1.0],
            interpolation=choice, edge="nearest",
        )  # fmt: skip
        errors[choice] = np.abs(out[1, 20:41, 0] - (reads / 60) ** 3).max()
    assert errors["bicubic"] < 5e-6
    assert errors["bilinear"] > 3e-5
    assert errors["nearest"] > errors["bilinear"]


def test_the_input_is_left_alone_and_the_result_keeps_its_shape_and_dtype() -> None:
    image = dot(4, 8)
    before = image.copy()
    out = accumulate_with(image, "spin", [0.3], [1.0])
    np.testing.assert_array_equal(image, before)
    assert out.shape == image.shape
    assert out.dtype == np.float32
