"""Behaviour of the `echo` effect: translucent copies of the picture, each offset a step further, laid over it."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "echo"


def impulse(height: int, width: int, row: int, column: int) -> np.ndarray:
    """A black picture with one white pixel."""
    rgb = np.zeros((height, width, 3), np.float32)
    rgb[row, column] = 1.0
    return rgb


def lit(rgb: np.ndarray) -> dict[tuple[int, int], float]:
    """The pixels of a grey-or-black picture that are not black, by (row, column), with their value."""
    return {(int(r), int(c)): float(rgb[r, c, 0]) for r, c in zip(*np.nonzero(rgb[..., 0]), strict=True)}


@pytest.mark.parametrize(
    ("setting", "expected"),
    [
        pytest.param({"copies": 0}, {(5, 6): 1.0}, id="no copies leave the picture as it is"),
        pytest.param(
            {"copies": 1, "dx": 3, "dy": 0, "fade": 0.0},
            {(5, 6): 0.5, (5, 9): 0.5},
            id="the first copy is the picture moved by (dx, dy), as strong as the original at fade 0",
        ),
        pytest.param(
            {"copies": 1, "dx": 0, "dy": -2, "fade": 0.0},
            {(5, 6): 0.5, (3, 6): 0.5},
            id="dy counts down: a negative dy puts the copy above",
        ),
        pytest.param(
            {"copies": 1, "dx": -2, "dy": 3, "fade": 0.5},
            {(5, 6): 2 / 3, (8, 4): 1 / 3},
            id="fade 0.5: the copy has half the weight of the original, so a third of the total",
        ),
        pytest.param(
            {"copies": 3, "dx": 3, "dy": 0, "fade": 0.5},
            {(5, 6): 1 / 1.875, (5, 9): 0.5 / 1.875, (5, 12): 0.25 / 1.875, (5, 15): 0.125 / 1.875},
            id="each copy is offset one step further and half as strong as the one before",
        ),
        pytest.param(
            {"copies": 2, "dx": 2, "dy": 1, "fade": 1.0},
            {(5, 6): 1.0},
            id="fade 1: the copies have faded away completely",
        ),
        pytest.param(
            {"copies": 1, "dx": 1.5, "dy": 0, "fade": 0.0},
            {(5, 6): 0.5, (5, 7): 0.25, (5, 8): 0.25},
            id="a fractional offset is shared by the two pixels it lies between",
        ),
    ],
)
def test_the_picture_is_averaged_with_copies_moved_a_step_further_and_weighted_by_fade_each(setting, expected) -> None:
    """A white pixel at row 5, column 6 of a black 12 x 20 picture. The original has weight 1, copy k the weight
    (1 - fade)^k, and the total weight is divided out, so the results are worked out by hand."""
    out = registry.apply(NAME, impulse(12, 20, 5, 6), edge="nearest", **setting)
    assert lit(out) == pytest.approx(expected)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [
        pytest.param("wrap", {(5, 18): 0.5, (5, 2): 0.5}, id="wrap: the copy that leaves comes back on the other side"),
        pytest.param("nearest", {(5, 18): 0.5}, id="nearest: the copy leaves the picture and is gone"),
        pytest.param("reflect", {(5, 18): 0.5}, id="reflect: the copy leaves the picture and is gone"),
        pytest.param("mirror", {(5, 18): 0.5}, id="mirror: the copy leaves the picture and is gone"),
    ],
)
def test_edge_decides_what_a_copy_that_leaves_the_picture_brings_back(edge: str, expected) -> None:
    out = registry.apply(NAME, impulse(12, 20, 5, 18), copies=1, dx=4, dy=0, fade=0.0, edge=edge)
    assert lit(out) == pytest.approx(expected)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [
        pytest.param("nearest", {(5, 0): 1.0, (5, 1): 0.5, (5, 2): 0.5, (5, 3): 0.5, (5, 4): 0.5}, id="nearest"),
        pytest.param("reflect", {(5, 0): 0.5, (5, 3): 0.5, (5, 4): 0.5}, id="reflect"),
        pytest.param("mirror", {(5, 0): 0.5, (5, 4): 0.5}, id="mirror"),
    ],
)
def test_the_gap_a_copy_opens_at_the_edge_is_filled_by_the_edge_rule(edge: str, expected) -> None:
    """A pixel on the first column, a copy moved 4 px right: its columns 0 to 3 lie beyond the left edge, where they
    read the picture's columns -4 to -1. Nearest reads column 0 there (so columns 0 to 4 of the copy show the pixel),
    reflect reads columns 3, 2, 1, 0 (the pixel is at column -1, so the copy shows it at 3, and at 4 from the real
    column 0), mirror reads 4, 3, 2, 1 (the pixel shows only at 4)."""
    out = registry.apply(NAME, impulse(12, 20, 5, 0), copies=1, dx=4, dy=0, fade=0.0, edge=edge)
    assert lit(out) == pytest.approx(expected)


@pytest.mark.parametrize("edge", ["wrap", "nearest", "reflect", "mirror"])
def test_a_flat_picture_stays_flat_whatever_the_copies(edge: str) -> None:
    out = registry.apply(NAME, np.full((9, 11, 3), 0.3, np.float32), copies=5, dx=7, dy=-4, fade=0.3, edge=edge)
    np.testing.assert_allclose(out, 0.3, atol=1e-6)
