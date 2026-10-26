"""Behaviour of the `quadtree` effect."""

import math

import numpy as np
import pytest

from bullterrier_image import fx
from bullterrier_image.core import registry
from bullterrier_image.errors import RecipeError

NAME = "quadtree"
RED = "#ff0000"
CELLS_OF_32_DOWN_TO_8 = {"max_cell": 0.5, "min_cell": 0.125}  # on a 64 px picture


def edge_at_column_40() -> np.ndarray:
    """64 x 64: black left of column 40, white from it on."""
    rgb = np.zeros((64, 64, 3), np.float32)
    rgb[:, 40:] = 1.0
    return rgb


def test_a_picture_whose_edges_fall_on_cell_borders_comes_back_unchanged() -> None:
    """The edge at column 40 is the border of two 8 px cells, so every cell the picture splits into is flat."""
    rgb = edge_at_column_40()
    out = registry.apply(NAME, rgb, threshold=0.1, gap=0.0, gap_color=RED, **CELLS_OF_32_DOWN_TO_8)
    np.testing.assert_array_equal(out, rgb)  # also: with no gap and no rounding, no red is drawn


def test_a_cell_that_straddles_the_edge_is_the_average_of_both_sides() -> None:
    """With cells that cannot split below 16 px, the cell over columns 32..48 holds 8 black and 8 white columns."""
    out = registry.apply(
        NAME, edge_at_column_40(), threshold=0.5, gap=0.0, max_cell=0.25, min_cell=0.25
    )  # 0.5 is the most a cell can spread and does not split: a plain grid of 16 px cells
    np.testing.assert_allclose(out[:, 32:48], 0.5, atol=1e-6)
    np.testing.assert_array_equal(out[:, :32], 0.0)
    np.testing.assert_array_equal(out[:, 48:], 1.0)


def test_the_average_colour_of_the_picture_is_kept(photo: np.ndarray) -> None:
    """The cells tile the picture and each holds its own average, so the picture's average cannot change."""
    out = registry.apply(NAME, photo, threshold=0.05, gap=0.0, max_cell=0.2, min_cell=0.02)
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=1e-5)


def test_a_lower_threshold_never_follows_the_picture_less_closely(photo: np.ndarray) -> None:
    """Splitting a cell replaces its average by the averages of its parts, which never fits the pixels worse."""
    errors = [
        float(((registry.apply(NAME, photo, threshold=t, gap=0.0, max_cell=0.5, min_cell=0.02) - photo) ** 2).mean())
        for t in (0.0, 0.05, 0.1, 0.2, 0.5)
    ]
    assert errors == sorted(errors)
    assert errors[0] < errors[-1]


@pytest.fixture
def two_by_two_cells() -> np.ndarray:
    """A flat grey 64 x 64 picture, which the first grid of 32 px cells covers with four cells."""
    return np.full((64, 64, 3), 0.5, np.float32)


def test_the_gap_is_a_line_centred_on_every_border_between_cells(two_by_two_cells: np.ndarray) -> None:
    """A 2 px gap on the border at 32 takes columns 31 and 32 and rows 31 and 32 whole, and nothing else."""
    out = registry.apply(NAME, two_by_two_cells, gap=2 / 64, gap_color=RED, max_cell=0.5, min_cell=0.5)
    line = np.zeros((64, 64), bool)
    line[31:33, :] = True
    line[:, 31:33] = True
    np.testing.assert_array_equal(out[line], np.broadcast_to(np.array([1, 0, 0], np.float32), (line.sum(), 3)))
    np.testing.assert_array_equal(out[~line], 0.5)


def test_the_gap_lines_fall_on_the_cell_borders_all_down_a_wide_picture() -> None:
    """A flat 200 x 1000 picture in cells of 100 px has borders at row 100 and at every hundredth column; a 2 px gap
    covers the two rows or columns beside each (99 and 100, 199 and 200, ...) and nothing else."""
    flat = np.full((200, 1000, 3), 0.5, np.float32)
    out = registry.apply(NAME, flat, gap=0.002, gap_color=RED, max_cell=0.1, min_cell=0.1)
    line = np.zeros((200, 1000), bool)
    line[99:101, :] = True
    line[:, [c for border in range(100, 1000, 100) for c in (border - 1, border)]] = True
    np.testing.assert_array_equal(out[..., 1] == 0.0, line)  # green is 0 on the red line and 0.5 elsewhere


def test_the_gap_does_not_draw_along_the_picture_edge(two_by_two_cells: np.ndarray) -> None:
    out = registry.apply(NAME, two_by_two_cells, gap=3 / 64, gap_color=RED, max_cell=0.5, min_cell=0.5)
    np.testing.assert_array_equal(out[0, :20], 0.5)  # the top row, away from the border at column 32
    np.testing.assert_array_equal(out[:20, 0], 0.5)
    np.testing.assert_array_equal(out[-1, -20:], 0.5)
    np.testing.assert_array_equal(out[-20:, -1], 0.5)


def test_full_rounding_turns_each_square_cell_into_a_disc_in_gap_colour(two_by_two_cells: np.ndarray) -> None:
    """A disc fills pi / 4 of its square. The soft rim of a one-pixel ramp moves that by about a pixel in 32."""
    out = registry.apply(NAME, two_by_two_cells, gap=0.0, rounding=1.0, gap_color=RED, max_cell=0.5, min_cell=0.5)
    grey_share = float((out[..., 1] > 0.25).mean())
    assert grey_share == pytest.approx(math.pi / 4, abs=0.03)
    assert tuple(out[16, 16]) == (0.5, 0.5, 0.5)  # a cell's centre is the cell's colour
    assert tuple(out[0, 0]) == (1.0, 0.0, 0.0)  # and its corner is cut away


def test_rounding_cuts_more_of_the_corners_the_higher_it_is(two_by_two_cells: np.ndarray) -> None:
    shares = [
        float(
            (
                registry.apply(NAME, two_by_two_cells, gap=0.0, rounding=r, gap_color=RED, max_cell=0.5, min_cell=0.5)[
                    ..., 1
                ]
                > 0.25
            ).mean()
        )
        for r in (0.0, 0.25, 0.5, 0.75, 1.0)
    ]
    assert shares[0] == 1.0
    assert shares == sorted(shares, reverse=True)


def test_cells_larger_than_the_picture_make_one_cell() -> None:
    rgb = np.random.default_rng(2).random((20, 30, 3), dtype=np.float32)
    out = registry.apply(NAME, rgb, threshold=0.5, gap=0.0, max_cell=1.0, min_cell=0.5)
    np.testing.assert_allclose(out, np.broadcast_to(rgb.mean(axis=(0, 1)), out.shape), atol=1e-6)


def test_a_smallest_cell_above_the_largest_is_refused_naming_both() -> None:
    with pytest.raises(RecipeError, match="min_cell") as raised:
        fx.quadtree(min_cell=0.2, max_cell=0.1)
    assert "max_cell" in str(raised.value)
    assert fx.quadtree(min_cell=0.1, max_cell=0.1).params["min_cell"] == 0.1  # equal is a range: a plain grid
