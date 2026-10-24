"""Tests of `ops/pieces.py`: how far pixels are from the borders between pieces, and a partition drawn with its pieces
numbered. The distances are checked against a hand computation (the borders of a grid are lines between pixels), the
drawing against what it promises: dimmed, white lines on the borders, a number inside each piece."""

import numpy as np
import pytest

from bullterrier_image.ops.pieces import border_distance, number_pieces


def grid_labels(widths: list[int], heights: list[int]) -> np.ndarray:
    """The pieces of a grid whose columns are `widths` px wide and rows `heights` px high, numbered row after row."""
    columns = np.repeat(np.arange(len(widths)), widths)
    rows = np.repeat(np.arange(len(heights)), heights)
    return (rows[:, None] * len(widths) + columns[None, :]).astype(np.int32)


def crack_distance(widths: list[int], heights: list[int]) -> np.ndarray:
    """From every pixel centre to the nearest line between two columns or two rows, worked out one pixel at a time."""
    across, down = np.cumsum(widths)[:-1], np.cumsum(heights)[:-1]
    x, y = np.arange(sum(widths)) + 0.5, np.arange(sum(heights)) + 0.5
    to_columns = np.min(np.abs(x[:, None] - across[None, :]), axis=1, initial=np.inf)
    to_rows = np.min(np.abs(y[:, None] - down[None, :]), axis=1, initial=np.inf)
    return np.minimum(to_rows[:, None], to_columns[None, :])


@pytest.mark.parametrize(
    ("widths", "heights"),
    [([3, 5, 4], [6]), ([7], [2, 9, 1]), ([3, 5, 4], [2, 9, 1]), ([1, 1, 1], [1, 1]), ([10, 1], [1, 10])],
    ids=["columns", "rows", "both", "one pixel each", "thin and wide"],
)
def test_a_pixel_is_as_far_from_a_border_as_the_nearest_line_between_two_pieces(widths, heights):
    np.testing.assert_allclose(border_distance(grid_labels(widths, heights)), crack_distance(widths, heights))


def test_a_single_piece_has_no_border_to_be_near():
    assert (border_distance(np.zeros((4, 5), np.int32)) == np.inf).all()


def exact_distance(labels: np.ndarray) -> np.ndarray:
    """From every pixel centre to the nearest unit segment between two pixels of different pieces, by brute force."""
    height, width = labels.shape
    segments = []  # (x0, y0, x1, y1)
    for y in range(height):
        for x in range(width):
            if x + 1 < width and labels[y, x] != labels[y, x + 1]:
                segments.append((x + 1, y, x + 1, y + 1))
            if y + 1 < height and labels[y, x] != labels[y + 1, x]:
                segments.append((x, y + 1, x + 1, y + 1))
    out = np.full(labels.shape, np.inf)
    for y in range(height):
        for x in range(width):
            for x0, y0, x1, y1 in segments:  # the segments are axis-aligned, so the nearest point is a clamp
                nearest_x, nearest_y = min(max(x + 0.5, x0), x1), min(max(y + 0.5, y0), y1)
                out[y, x] = min(out[y, x], np.hypot(x + 0.5 - nearest_x, y + 0.5 - nearest_y))
    return out


def test_along_a_slanting_border_the_distance_is_within_a_pixel_of_the_exact_one():
    labels = (np.arange(11)[None, :] >= np.arange(11)[:, None]).astype(np.int32)  # a diagonal border
    exact, found = exact_distance(labels), border_distance(labels)
    assert np.abs(found - exact).max() <= 1
    np.testing.assert_allclose(found[exact == 0.5], 0.5)  # a pixel beside the border is half a pixel from it


@pytest.mark.parametrize("size", [(1, 1), (1, 7), (7, 1), (13, 29), (96, 128)], ids=lambda s: f"{s[0]}x{s[1]}")
def test_a_numbered_partition_is_a_dimmed_copy_in_range_whatever_the_size_and_leaves_the_input_alone(size):
    rgb = np.random.default_rng(4).random((*size, 3), dtype=np.float32)
    kept = rgb.copy()
    labels = grid_labels([size[1] // 3 + 1, size[1] - size[1] // 3 - 1] if size[1] > 2 else [size[1]], [size[0]])
    out = number_pieces(rgb, labels, int(labels.max()) + 1)
    assert out.shape == rgb.shape
    assert out.dtype == np.float32
    assert out.min() >= 0
    assert out.max() <= 1
    np.testing.assert_array_equal(rgb, kept)


def test_pixels_far_from_every_border_and_number_are_the_photo_dimmed_and_the_borders_are_white():
    rgb = np.random.default_rng(5).random((96, 128, 3), dtype=np.float32)
    labels = grid_labels([64, 64], [96])
    out = number_pieces(rgb, labels, 2)
    np.testing.assert_allclose(out[:8, :32], rgb[:8, :32] * 0.35, rtol=1e-6)  # the top left corner: nothing drawn there
    assert (out[:, 63:65] == 1).all()  # the two columns beside the border
    assert (out[:, 61] != 1).any()


def test_every_piece_has_its_number_and_more_pieces_make_wider_numbers():
    rgb = np.zeros((60, 600, 3), np.float32)
    few = number_pieces(rgb, grid_labels([300, 300], [60]), 2)
    many = number_pieces(rgb, grid_labels([30] * 20, [60]), 20)
    assert (few[:, :290].max(axis=2) > 0.5).any()  # a number in the first piece
    assert (few[:, 310:].max(axis=2) > 0.5).any()  # and in the second
    for piece in range(20):  # numbers 10 to 19 have two digits and must fit their piece's centre
        assert (many[:, piece * 30 + 3 : piece * 30 + 27].max(axis=2) > 0.5).any(), piece
