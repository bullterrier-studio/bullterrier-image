"""Behaviour of the `pixel_grid` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "pixel_grid"


def white(size: int) -> np.ndarray:
    return np.ones((size, size, 3), np.float32)


def test_amount_0_leaves_the_picture_unchanged_and_amount_blends_linearly_to_the_full_grid(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, amount=0), photo)
    full = registry.apply(NAME, photo, cell=5, gap=0.3, amount=1)
    half = registry.apply(NAME, photo, cell=5, gap=0.3, amount=0.5)
    np.testing.assert_allclose(half, 0.5 * photo + 0.5 * full, atol=1e-6)


def test_cells_without_a_gap_are_three_stripes_each_showing_one_channel() -> None:
    """Cell 6 = stripes 2 px wide: red in columns 0-1, green in 2-3, blue in 4-5, the same in every row."""
    out = registry.apply(NAME, white(12), cell=6, gap=0, amount=1)
    stripes = np.array([[1, 0, 0]] * 2 + [[0, 1, 0]] * 2 + [[0, 0, 1]] * 2, np.float32)
    np.testing.assert_array_equal(out[3], np.tile(stripes, (2, 1)))
    np.testing.assert_array_equal(out, np.broadcast_to(out[:1], out.shape))


def test_a_gap_darkens_the_edges_of_each_stripe_and_the_edge_between_rows_of_cells() -> None:
    """Cell 6, gap 0.5: a stripe is 2 px wide and its dark gap 1 px (half on each side), vertically too. The first red
    stripe lights 0.5 of column 0 and 0.5 of column 1, and 0.5 of the cell's first row and last row."""
    out = registry.apply(NAME, white(12), cell=6, gap=0.5, amount=1)
    half = 0.5
    np.testing.assert_allclose(
        out[2, :6],
        [[half, 0, 0], [half, 0, 0], [0, half, 0], [0, half, 0], [0, 0, half], [0, 0, half]],
        atol=1e-6,
    )
    np.testing.assert_allclose(out[0, :2, 0], [half * half, half * half], atol=1e-6)  # top row of a cell, red stripe
    np.testing.assert_allclose(out[5, :2, 0], [half * half, half * half], atol=1e-6)  # bottom row of a cell
    np.testing.assert_allclose(out[6, :2, 0], out[0, :2, 0], atol=1e-6)  # the next cell starts the same way


@pytest.mark.parametrize(
    ("cell", "gap", "channel_mean"),
    [
        pytest.param(6, 0.0, 1 / 3, id="no gap: each channel lit a third of the time"),
        pytest.param(6, 0.25, (2 - 0.5) * (6 - 0.5) / 36, id="gap 0.25 of a 2 px stripe is 0.5 px, lost in both axes"),
        pytest.param(9, 0.6, (3 - 1.8) * (9 - 1.8) / 81, id="gap 0.6 of a 3 px stripe"),
        pytest.param(4, 1.0, 0.0, id="gap 1 leaves nothing lit"),
    ],
)
def test_each_channel_is_lit_over_the_stripe_area_the_gap_leaves(cell: int, gap: float, channel_mean: float) -> None:
    """On white, over whole cells: a channel's mean is its stripe (cell/3 wide, cell tall) less a dark border."""
    out = registry.apply(NAME, white(4 * cell), cell=cell, gap=gap, amount=1)
    np.testing.assert_allclose(out.mean(axis=(0, 1)), channel_mean, atol=1e-6)


def test_the_pattern_repeats_with_the_period_of_the_cell() -> None:
    out = registry.apply(NAME, white(35), cell=7, gap=0.3, amount=1)
    np.testing.assert_allclose(out[7:28, 7:28], out[:21, :21], atol=1e-6)
    assert not np.allclose(out[1:8, 1:8], out[:7, :7])  # and not with a shorter one
