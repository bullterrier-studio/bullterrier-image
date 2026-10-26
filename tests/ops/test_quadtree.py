"""Tests of `ops/quadtree.py`: the cells a picture is cut into, against hand counts and a brute-force reference written
here."""

import numpy as np
import pytest

from bullterrier_image.ops.quadtree import Leaves, leaves


def edge_at_column_40() -> np.ndarray:
    """64 x 64: black left of column 40, white from it on."""
    rgb = np.zeros((64, 64, 3), np.float32)
    rgb[:, 40:] = 1.0
    return rgb


def spread_of(rgb: np.ndarray, box: np.ndarray) -> float:
    """The largest standard deviation over the channels of the pixels inside `box` = (top, bottom, left, right)."""
    top, bottom, left, right = box
    return float(rgb[top:bottom, left:right].astype(np.float64).reshape(-1, 3).std(axis=0).max())


def sizes(found: Leaves) -> dict[int, int]:
    """How many leaves of each area (px) there are."""
    area = (found.box[:, 1] - found.box[:, 0]) * (found.box[:, 3] - found.box[:, 2])
    values, counts = np.unique(area, return_counts=True)
    return dict(zip(values.tolist(), counts.tolist(), strict=True))


def test_a_hard_edge_splits_only_the_cells_it_crosses_down_to_the_smallest() -> None:
    """Hand count. Level 0 is four 32 px cells; the two on the right cross column 40, so each splits into four 16 px
    cells. Of those, the two over columns 32..48 cross it again and split into 8 px cells; the two over columns 48..64
    are flat. So per right-hand cell: 2 sixteens and 8 eights; the two of them give 4 sixteens and 16 eights, and the
    two flat left-hand cells stay at 32 px."""
    found = leaves(edge_at_column_40(), threshold=0.1, max_cell=32, min_cell=8)
    assert sizes(found) == {32 * 32: 2, 16 * 16: 4, 8 * 8: 16}


def test_a_change_in_one_channel_alone_is_enough_to_split_a_cell() -> None:
    red_edge = np.zeros((64, 64, 3), np.float32)
    red_edge[:, 40:, 0] = 1.0
    assert sizes(leaves(red_edge, threshold=0.1, max_cell=32, min_cell=8)) == {32 * 32: 2, 16 * 16: 4, 8 * 8: 16}


def test_a_flat_picture_stays_in_the_largest_cells() -> None:
    found = leaves(np.full((64, 96, 3), 0.4, np.float32), threshold=0.0, max_cell=32, min_cell=4)
    assert sizes(found) == {32 * 32: 6}


@pytest.mark.parametrize("value", [0.1, 0.3, 0.7, 0.9], ids=lambda v: f"flat {v}")
def test_a_large_flat_picture_of_any_brightness_is_never_called_uneven(value: float) -> None:
    """Spreads come from sums of squares, which at a million pixels drown the tiny differences of a bright flat picture
    unless the values are centred: it must stay in the first grid (1000 px in cells of at most 125 is 8 x 8)."""
    flat = np.full((1000, 1000, 3), value, np.float32)
    assert len(leaves(flat, threshold=0.0, max_cell=125, min_cell=4).box) == 8 * 8


def test_a_picture_one_pixel_high_cannot_split() -> None:
    """A cell splits only into halves of at least `min_cell` px a side, and half of one pixel is none."""
    noise = np.random.default_rng(9).random((1, 64, 3), dtype=np.float32)
    assert len(leaves(noise, threshold=0.0, max_cell=64, min_cell=1).box) == 1


def test_noise_splits_down_to_the_smallest_cells_when_the_threshold_is_zero() -> None:
    noise = np.random.default_rng(3).random((64, 64, 3), dtype=np.float32)
    assert sizes(leaves(noise, threshold=0.0, max_cell=32, min_cell=4)) == {4 * 4: 16 * 16}


def test_a_threshold_above_any_spread_keeps_the_largest_cells() -> None:
    noise = np.random.default_rng(3).random((64, 64, 3), dtype=np.float32)
    assert sizes(leaves(noise, threshold=0.5, max_cell=16, min_cell=2)) == {16 * 16: 16}


@pytest.mark.parametrize("shape", [(37, 53), (64, 64), (5, 200), (1, 1), (23, 7)], ids=lambda s: f"{s[0]}x{s[1]}")
def test_the_leaves_tile_the_picture_and_each_holds_the_mean_of_its_pixels(shape: tuple[int, int]) -> None:
    rgb = np.random.default_rng(5).random((*shape, 3), dtype=np.float32)
    found = leaves(rgb, threshold=0.2, max_cell=16, min_cell=2)
    area = (found.box[:, 1] - found.box[:, 0]) * (found.box[:, 3] - found.box[:, 2])
    assert area.min() > 0
    assert area.sum() == shape[0] * shape[1]  # no overlap and no hole follow from covering every pixel once
    assert found.label.shape == shape
    for index, (top, bottom, left, right) in enumerate(found.box):
        inside = np.zeros(shape, bool)
        inside[top:bottom, left:right] = True
        np.testing.assert_array_equal(found.label == index, inside)  # the label picture and the boxes agree
        np.testing.assert_allclose(
            found.mean[index], rgb[top:bottom, left:right].reshape(-1, 3).mean(axis=0), atol=1e-5
        )


def test_no_leaf_that_could_split_is_left_above_the_threshold() -> None:
    rgb = (
        np.random.default_rng(7).random((64, 64, 3), dtype=np.float32)
        * np.linspace(0, 1, 64, dtype=np.float32)[None, :, None]
    )
    threshold, min_cell = 0.12, 4
    found = leaves(rgb, threshold=threshold, max_cell=32, min_cell=min_cell)
    for box in found.box:
        small = min(box[1] - box[0], box[3] - box[2]) // 2 < min_cell  # its halves would be below the smallest cell
        assert small or spread_of(rgb, box) <= threshold, box


def test_raising_the_threshold_never_gives_more_leaves() -> None:
    rgb = (
        np.random.default_rng(11).random((64, 64, 3), dtype=np.float32)
        * np.linspace(0, 1, 64, dtype=np.float32)[None, :, None]
    )
    counts = [len(leaves(rgb, threshold=t, max_cell=32, min_cell=2).box) for t in (0.0, 0.05, 0.1, 0.2, 0.3, 0.5)]
    assert counts == sorted(counts, reverse=True)
    assert counts[0] > counts[-1]


def test_the_smallest_cell_is_never_smaller_than_asked_even_on_an_odd_picture() -> None:
    rgb = np.random.default_rng(13).random((37, 53, 3), dtype=np.float32)
    found = leaves(rgb, threshold=0.0, max_cell=20, min_cell=5)
    heights = found.box[:, 1] - found.box[:, 0]
    widths = found.box[:, 3] - found.box[:, 2]
    assert heights.min() >= 5
    assert widths.min() >= 5
    assert heights.max() <= 20
    assert widths.max() <= 20


def test_the_picture_is_not_modified() -> None:
    rgb = np.random.default_rng(17).random((40, 40, 3), dtype=np.float32)
    before = rgb.copy()
    leaves(rgb, threshold=0.1, max_cell=16, min_cell=2)
    np.testing.assert_array_equal(rgb, before)
