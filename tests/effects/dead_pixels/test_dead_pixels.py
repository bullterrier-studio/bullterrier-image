"""Behaviour of the `dead_pixels` effect: seeded stuck (bright) and dead (black) pixels on a grid of cells."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "dead_pixels"
GREY = 0.5  # a picture of this grey shows a stuck pixel (white or a colour) and a dead one (black) against it


def grey(height: int, width: int) -> np.ndarray:
    return np.full((height, width, 3), GREY, np.float32)


def changed(out: np.ndarray, picture: np.ndarray) -> np.ndarray:
    """(height, width) mask of the pixels that differ from the picture in any channel."""
    return (out != picture).any(axis=-1)


@pytest.mark.parametrize("count", [0, 1, 7, 100])
@pytest.mark.parametrize("stuck", [0.0, 0.5, 1.0])
def test_exactly_count_pixels_change_and_the_rest_are_untouched(count: int, stuck: float) -> None:
    picture = grey(40, 50)
    out = registry.apply(NAME, picture, count=count, size=1, stuck=stuck, seed=3)
    assert changed(out, picture).sum() == count
    np.testing.assert_array_equal(out[~changed(out, picture)], picture[~changed(out, picture)])


@pytest.mark.parametrize(
    ("stuck", "bright", "dark"),
    [(0.0, 0, 10), (1.0, 10, 0), (0.5, 5, 5), (0.3, 3, 7), (0.26, 3, 7)],
    ids=["none stuck", "all stuck", "half stuck", "three tenths stuck", "2.6 stuck rounds to 3"],
)
def test_stuck_is_the_share_of_the_defects_that_are_the_colour_and_the_rest_are_black(
    stuck: float, bright: int, dark: int
) -> None:
    picture = grey(40, 50)
    out = registry.apply(NAME, picture, count=10, size=1, stuck=stuck, color="#ff8000", seed=1)
    stuck_mask = (out == np.array([1.0, 128 / 255, 0.0], np.float32)).all(axis=-1)
    dead_mask = (out == 0).all(axis=-1)
    assert (stuck_mask.sum(), dead_mask.sum()) == (bright, dark)


@pytest.mark.parametrize("size", [2, 3, 5])
def test_a_defect_fills_one_square_cell_of_a_grid_of_size_px(size: int) -> None:
    """On a 30 x 30 picture with size 2, 3 or 5 the grid cells are 15, 10 or 6 per side; each cell is all one defect or
    untouched, and as many cells as `count` are."""
    picture = grey(30, 30)
    cells = 30 // size
    out = registry.apply(NAME, picture, count=6, size=size, seed=2)
    blocks = changed(out, picture).reshape(cells, size, cells, size)
    per_cell = blocks.sum(axis=(1, 3))
    assert set(np.unique(per_cell)) <= {0, size * size}
    assert (per_cell > 0).sum() == 6


def test_a_count_beyond_the_cells_fills_every_cell_including_the_clipped_ones_at_the_edge() -> None:
    """A 7 x 5 picture with size 3 has 3 x 2 cells (the last row and column of cells are 1 and 2 px wide)."""
    picture = grey(7, 5)
    out = registry.apply(NAME, picture, count=10000, size=3, stuck=1.0, color="#ffffff")
    assert (out == 1.0).all()


def test_the_same_defects_are_chosen_whatever_the_colour_and_share() -> None:
    """Seed and count alone choose where the defects are: the colour and the share only paint them."""
    picture = grey(40, 50)
    first = changed(registry.apply(NAME, picture, count=20, stuck=1.0, color="#ff0000", seed=4), picture)
    second = changed(registry.apply(NAME, picture, count=20, stuck=0.0, seed=4), picture)
    np.testing.assert_array_equal(first, second)


def test_more_defects_keep_the_ones_there_were() -> None:
    """Raising the count only adds defects: animating it never moves one that is already there."""
    picture = grey(40, 50)
    few = changed(registry.apply(NAME, picture, count=10, seed=5), picture)
    many = changed(registry.apply(NAME, picture, count=25, seed=5), picture)
    assert (many | few).sum() == many.sum()
    assert few.sum() < many.sum()
