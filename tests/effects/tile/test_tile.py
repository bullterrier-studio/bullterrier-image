"""Behaviour of the `tile` effect: the whole picture, scaled down, repeated in a grid of columns by rows."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "tile"


def grey(values) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(np.asarray(values, np.float32)[..., None], 3, axis=2)


def test_a_two_by_two_grid_repeats_the_picture_at_half_size_with_each_pixel_the_mean_of_a_block() -> None:
    """The 4 x 4 picture 0..15 scaled to 2 x 2 has the mean of each 2 x 2 block: (0 + 1 + 4 + 5) / 4 = 2.5 and so on."""
    picture = grey(np.arange(16).reshape(4, 4) / 20)
    cell = np.array([[2.5, 4.5], [10.5, 12.5]]) / 20
    out = registry.apply(NAME, picture, columns=2, rows=2)
    np.testing.assert_allclose(out[..., 0], np.tile(cell, (2, 2)), atol=1e-6)


def test_columns_count_across_and_rows_count_down() -> None:
    """Two rows of six columns into 3 columns by 1 row: cells of 2 x 2 px, each pixel the mean of 1 row x 3 columns,
    so (0 + 1 + 2) / 3 = 1, 4, and 7, 10 on the second row (values 0..11, divided by 12)."""
    picture = grey(np.arange(12).reshape(2, 6) / 12)
    cell = np.array([[1, 4], [7, 10]]) / 12
    out = registry.apply(NAME, picture, columns=3, rows=1)
    np.testing.assert_allclose(out[..., 0], np.tile(cell, (1, 3)), atol=1e-6)


def test_the_pixels_left_over_by_a_grid_that_does_not_divide_show_the_start_of_the_next_tile() -> None:
    """Five columns into two cells of 2 px: each cell covers 2.5 source columns, so the first is
    (0 + 1 + half of 2) / 2.5 = 0.8 and the second (half of 2 + 3 + 4) / 2.5 = 3.2 (values 0..4, divided by 10); the
    fifth column is the first column of a third, cut-off tile."""
    out = registry.apply(NAME, grey([[0, 1, 2, 3, 4]]) / 10, columns=2, rows=1)
    np.testing.assert_allclose(out[0, :, 0], np.array([0.8, 3.2, 0.8, 3.2, 0.8]) / 10, atol=1e-6)


@pytest.mark.parametrize(("columns", "rows"), [(4, 3), (2, 2), (1, 6), (32, 96)])
def test_on_a_picture_the_grid_divides_every_tile_is_the_picture_in_blocks_of_mean_values(columns, rows, photo) -> None:
    """Each tile pixel is the plain mean of a block of `rows` x `columns` source pixels (brute force by reshaping)."""
    cell = photo.reshape(96 // rows, rows, 128 // columns, columns, 3).mean(axis=(1, 3), dtype=np.float64)
    out = registry.apply(NAME, photo, columns=columns, rows=rows)
    np.testing.assert_allclose(out, np.tile(cell, (rows, columns, 1)), atol=1e-6)


def test_a_single_cell_is_the_picture_itself(photo) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, columns=1, rows=1), photo)


@pytest.mark.parametrize(("columns", "rows"), [(1, 1), (3, 3), (5, 2)])
def test_a_flat_picture_stays_flat(columns: int, rows: int) -> None:
    out = registry.apply(NAME, np.full((31, 47, 3), 0.4, np.float32), columns=columns, rows=rows)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)
