"""Behaviour of the `tile` effect: the whole picture, scaled down, repeated in a grid of columns by rows."""

import numpy as np
import pytest

from bullterrier_image.core import registry

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


def impulse(height: int, width: int, row: int, column: int) -> np.ndarray:
    """A black picture with one white pixel."""
    rgb = np.zeros((height, width, 3), np.float32)
    rgb[row, column] = 1.0
    return rgb


def lit(rgb: np.ndarray) -> dict[tuple[int, int], float]:
    """The pixels of a picture that are not black, by (row, column), with their value in the first channel."""
    return {(int(r), int(c)): float(rgb[r, c, 0]) for r, c in zip(*np.nonzero(rgb[..., 0]), strict=True)}


@pytest.mark.parametrize("offset", [0.0, 1.0, -1.0])
@pytest.mark.parametrize("grid", [(1, 1), (4, 3)])
def test_no_offset_and_a_whole_tile_offset_leave_the_tiling_as_it_is(offset: float, grid, photo) -> None:
    columns, rows = grid
    plain = registry.apply(NAME, photo, columns=columns, rows=rows)
    moved = registry.apply(NAME, photo, columns=columns, rows=rows, offset_x=offset, offset_y=offset)
    np.testing.assert_allclose(moved, plain, atol=1e-6)


@pytest.mark.parametrize(
    ("offsets", "shift", "axis"),
    [
        pytest.param({"offset_y": 0.25}, 24, 0, id="y: a quarter of 96 rows is 24, down"),
        pytest.param({"offset_y": -0.25}, -24, 0, id="y: negative goes up"),
        pytest.param({"offset_x": 0.5}, 64, 1, id="x: half of 128 columns is 64, right"),
        pytest.param({"offset_x": -0.125}, -16, 1, id="x: negative goes left"),
        pytest.param({"offset_y": 0.75}, 72, 0, id="y: what leaves at the bottom wraps to the top"),
    ],
)
def test_one_tile_with_an_offset_is_the_picture_rolled_around(offsets, shift: int, axis: int, photo) -> None:
    out = registry.apply(NAME, photo, columns=1, rows=1, **offsets)
    np.testing.assert_allclose(out, np.roll(photo, shift, axis=axis), atol=1e-6)


def test_an_offset_is_a_fraction_of_a_tile_and_moves_every_tile_alike(photo) -> None:
    """A 4 x 2 grid on 128 x 96 has tiles of 32 x 48 px: 0.25 is 8 columns, 0.5 is 24 rows."""
    plain = registry.apply(NAME, photo, columns=4, rows=2)[:48, :32]
    out = registry.apply(NAME, photo, columns=4, rows=2, offset_x=0.25, offset_y=0.5)
    np.testing.assert_allclose(out, np.tile(np.roll(plain, (24, 8), axis=(0, 1)), (2, 4, 1)), atol=1e-6)


@pytest.mark.parametrize(
    ("offset", "blend"),
    [
        pytest.param(0.125, {(1, 6): 0.5, (1, 7): 0.5}, id="2.5 px: half on each of two pixels"),
        pytest.param(0.0625, {(1, 5): 0.75, (1, 6): 0.25}, id="1.25 px: three quarters on the nearer pixel"),
        pytest.param(-0.0625, {(1, 2): 0.25, (1, 3): 0.75}, id="-1.25 px: leftwards, a quarter on the farther pixel"),
    ],
)
def test_an_offset_that_ends_between_pixels_shares_the_pixel_between_the_two_it_lies_between(offset, blend) -> None:
    """On 20 columns in one tile the offset is 20 x offset px; a pixel at column 4 lands at 4 + that, the farther
    pixel taking the part beyond a whole pixel (linear interpolation, worked out by hand)."""
    out = registry.apply(NAME, impulse(3, 20, 1, 4), columns=1, rows=1, offset_x=offset)
    assert lit(out) == pytest.approx(blend)


def test_an_offset_keeps_a_flat_picture_flat() -> None:
    out = registry.apply(NAME, np.full((31, 47, 3), 0.4, np.float32), columns=3, rows=2, offset_x=0.37, offset_y=-0.6)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_the_scan_roll_preset_is_the_picture_rolled_down_a_quarter(photo) -> None:
    """The rolling scan of a slipped sync, once an effect of its own: one tile, wrapped around."""
    out = registry.apply(NAME, photo, **registry.presets(NAME)["scan_roll"].params)
    np.testing.assert_allclose(out, np.roll(photo, 24, axis=0), atol=1e-6)
