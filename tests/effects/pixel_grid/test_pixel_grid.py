"""Behaviour of the `pixel_grid` effect."""

import numpy as np
import pytest

from bullterrier_image import fx, render
from bullterrier_image.core import registry

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


@pytest.mark.parametrize(
    ("cell", "gap", "amount"),
    [
        pytest.param(3, 0.0, 0.5, id="stripes one pixel wide, no gap"),
        pytest.param(6, 0.25, 0.8, id="a gap"),
        pytest.param(4, 0.3, 0.6, id="a cell that is not a whole number of pixels a stripe"),
        pytest.param(9, 0.0, 1.0, id="the full grid"),
    ],
)
def test_keep_brightness_keeps_the_average_of_a_dim_picture_over_whole_cells(
    cell: int, gap: float, amount: float
) -> None:
    """Over whole cells the gains average 1 - amount x (1 - the lit share), which they are divided by; a grey of 0.2
    stays below 1 whatever it is lifted by, so nothing clips and the picture averages 0.2."""
    grey = np.full((4 * cell, 4 * cell, 3), 0.2, np.float32)
    out = registry.apply(NAME, grey, cell=cell, gap=gap, amount=amount, keep_brightness=True)
    assert out.mean() == pytest.approx(0.2, abs=1e-5)
    assert out.std() > 0.01  # and the grid still shows
    assert registry.apply(NAME, grey, cell=cell, gap=gap, amount=amount).mean() < 0.2  # where it dims without


def test_keep_brightness_lifts_a_lit_stripe_by_the_mean_darkening() -> None:
    """Cell 3, no gap, amount 0.5: a lit stripe passes all the light and an unlit one half, the mean gain is
    (1 + 0.5 + 0.5) / 3 = 2/3, so the gains are 1.5 and 0.75:
    a grey of 0.4 shows 0.6 on the lit stripe, 0.3 elsewhere."""
    out = registry.apply(NAME, np.full((3, 6, 3), 0.4, np.float32), cell=3, gap=0, amount=0.5, keep_brightness=True)
    expected = np.full((3, 6, 3), 0.3, np.float32)
    for column in range(6):
        expected[:, column, column % 3] = 0.6
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_keep_brightness_gives_the_same_stripes_whatever_the_size_of_the_picture() -> None:
    """The mean is over a cell, not over the picture: a smaller picture is the top left of a larger one."""
    noise = np.random.default_rng(4).random((30, 50, 3), dtype=np.float32)
    small = registry.apply(NAME, noise[:11, :17], cell=5, gap=0.2, amount=0.7, keep_brightness=True)
    large = registry.apply(NAME, noise, cell=5, gap=0.2, amount=0.7, keep_brightness=True)
    np.testing.assert_allclose(small, large[:11, :17], atol=1e-6)


def test_keep_brightness_with_nothing_lit_is_black_and_with_amount_0_changes_nothing(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, gap=1.0, amount=1.0, keep_brightness=True), 0, atol=1e-6)
    np.testing.assert_array_equal(registry.apply(NAME, photo, amount=0.0, keep_brightness=True), photo)


def test_keep_brightness_clips_a_lifted_stripe_at_white() -> None:
    """Cell 3, no gap, amount 0.5 lifts a lit stripe by 1.5: white stays 1 there and is 0.75 on the other stripes."""
    out = registry.apply(NAME, white(6), cell=3, gap=0, amount=0.5, keep_brightness=True)
    np.testing.assert_allclose(out[0, :3], [[1, 0.75, 0.75], [0.75, 1, 0.75], [0.75, 0.75, 1]], atol=1e-6)


def test_rendered_at_a_small_size_the_cell_shrinks_to_its_floor_as_a_whole_number() -> None:
    """At size 64 the default 4 px cell scales to 4 * 64 / 480 = 0.53 px, below the minimum of 3, so it is 3: stripes
    one pixel wide, in the order red, green, blue along every row (with no gap and amount 1)."""
    rng = np.random.default_rng(0)
    photo = rng.random((48, 64, 3), dtype=np.float32)
    out = render(photo, fx.pixel_grid(gap=0, amount=1), size=64)
    expected = np.zeros_like(photo)
    for channel in range(3):
        expected[:, channel::3, channel] = photo[:, channel::3, channel]
    np.testing.assert_allclose(out, expected, atol=1e-6)


@pytest.mark.parametrize("keep_brightness", [False, True], ids=["dimmed", "brightness kept"])
@pytest.mark.parametrize("cell", range(3, 33))
def test_a_gap_of_1_lights_nothing_whatever_the_cell_so_the_picture_is_black(cell: int, keep_brightness: bool) -> None:
    """`gap` 1 is the documented maximum: the dark lines are as wide as the stripes, so no stripe has any area. A third
    of a cell is not a whole number of pixels for most cells, which is where a width computed by subtraction rounds."""
    out = registry.apply(NAME, white(48), cell=cell, gap=1, amount=1, keep_brightness=keep_brightness)
    np.testing.assert_array_equal(out, np.zeros_like(out))


def test_with_no_settings_a_cell_of_4_px_lights_each_stripe_as_the_documented_gap_and_amount_leave_it() -> None:
    """Hand-worked defaults on a white picture: cell 4, gap 0.15, amount 0.4. A stripe is 4 / 3 px wide and the gap
    leaves 0.15 * (4 / 3) / 2 = 0.1 px free on each side of it, so red covers 0.1 to 1.2333 px of the cell, green 1.4333
    to 2.5667 and blue 2.7667 to 3.9; a pixel's lit share is the length of that it spans. A row of cells likewise
    covers 0.1 to 3.9 of its 4 px. The result is the picture mixed with the grid by amount: 1 - 0.4 * (1 - lit)."""
    out = registry.apply(NAME, white(8))
    lit_columns = np.array(  # red, green, blue shares of the 4 pixels of a cell, left to right
        [[0.9, 0, 0], [0.2333333, 0.5666667, 0], [0, 0.5666667, 0.2333333], [0, 0, 0.9]]
    )
    for row, lit_row in [(0, 0.9), (1, 1.0), (2, 1.0), (3, 0.9), (4, 0.9)]:  # row 4 starts the next cell
        expected = 1 - 0.4 * (1 - lit_row * lit_columns)
        np.testing.assert_allclose(out[row], np.tile(expected, (2, 1)), atol=1e-6, err_msg=f"row {row}")
