"""Quadtree cells: a picture cut into rectangles that stay large where it is even and halve where it varies.

Coordinates are pixel indices; a box `(top, bottom, left, right)` holds the rows `top..bottom - 1` and the columns
`left..right - 1`.
"""

from typing import NamedTuple

import numpy as np


class Leaves(NamedTuple):
    """The cells (leaves) a picture was cut into."""

    label: np.ndarray  # int32 (height, width): the index of the leaf each pixel belongs to
    box: np.ndarray  # int (count, 4): top, bottom, left, right of each leaf, in px
    mean: np.ndarray  # float32 (count, 3): the average colour of the picture inside each leaf


def _tables(rgb: np.ndarray, centre: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The running sums of the values of `rgb` (height, width, 3) minus `centre` (3 numbers), and of their squares, down
    the rows and along the columns, each with a leading row and column of zeros: the sum over any block is then four
    lookups. float64, one channel at a time (a picture-sized copy of one channel is all the memory it takes), so a
    million pixels add up without losing the small differences a spread is made of."""
    height, width, channels = rgb.shape
    sums = np.zeros((height + 1, width + 1, channels), np.float64)
    squares = np.zeros_like(sums)
    for channel in range(channels):
        plane = rgb[..., channel] - centre[channel]
        for table, values in ((sums, plane), (squares, plane**2)):
            running = table[1:, 1:, channel]
            np.cumsum(values, axis=0, out=running)
            np.cumsum(running, axis=1, out=running)
    return sums, squares


def _edges(length: int, count: int) -> np.ndarray:
    """Where `count` blocks of nearly equal size cut `length` px: `count + 1` rising indices from 0 to `length`, each at
    the whole px at or below its multiple of length / count (block sizes differ by one px at most). Exact integer
    arithmetic, so every edge of a coarse level is also an edge of the level that halves it."""
    return np.arange(count + 1, dtype=np.int64) * length // count


def _halves_fit(length: int, count: int, smallest: int) -> np.ndarray:
    """For each of the `count` blocks of `_edges(length, count)`, whether both its halves (the blocks of `_edges(length,
    2 * count)` inside it) are at least `smallest` px: a block of 2 x `smallest` px or more."""
    halves = np.diff(_edges(length, 2 * count))
    return np.minimum(halves[0::2], halves[1::2]) >= smallest


def _block_sums(table: np.ndarray, rows: np.ndarray, cols: np.ndarray) -> np.ndarray:
    """The sum of the summed `table`'s values over every block between the edges `rows` and `cols`, (blocks down, blocks
    across, 3)."""
    top, bottom, left, right = rows[:-1, None], rows[1:, None], cols[None, :-1], cols[None, 1:]
    return table[bottom, right] - table[top, right] - table[bottom, left] + table[top, left]


def leaves(rgb: np.ndarray, *, threshold: float, max_cell: int, min_cell: int) -> Leaves:
    """Cut `rgb` (a float32 height x width x 3 picture, 0..1) into leaves that follow its detail.

    The first cut is a grid of cells at most `max_cell` px on a side (the picture split evenly, so sides differ by a
    pixel at most). A cell splits into four halves when the spread of colour inside it, the largest of the three
    channels' standard deviations, is above `threshold`, and its halves are still at least `min_cell` px on a side (the
    cell is at least twice `min_cell` wide and high, each cell judged by its own size); the halves are tested the same
    way. `max_cell` and `min_cell` are whole px, 1 or more, `min_cell` not above `max_cell`.
    A picture smaller than a cell is one leaf. The picture is not modified.
    """
    height, width = rgb.shape[:2]
    # Spreads come from sums of squares, which lose the small differences when the values are large against them (a
    # flat 0.7 would read as uneven); centring on the picture's own average keeps the values near zero.
    centre = rgb.mean(axis=(0, 1), dtype=np.float64)
    sums, squares = _tables(rgb, centre)
    label = np.zeros((height, width), np.int32)
    boxes: list[np.ndarray] = []
    means: list[np.ndarray] = []
    down, across = -(-height // max_cell), -(-width // max_cell)
    active = np.ones((down, across), bool)  # the cells that exist: their parent split
    while active.any():
        rows, cols = _edges(height, down), _edges(width, across)
        # A cell of one px does not split, but its four "halves" are listed (two of them empty); nothing reads those.
        area = np.maximum(np.diff(rows)[:, None] * np.diff(cols)[None, :], 1)[..., None]
        mean = _block_sums(sums, rows, cols) / area  # relative to `centre`
        variance = _block_sums(squares, rows, cols) / area - mean**2
        spread = np.sqrt(np.maximum(variance, 0.0).max(axis=-1))
        halves_fit = _halves_fit(height, down, min_cell)[:, None] & _halves_fit(width, across, min_cell)[None, :]
        split = active & (spread > threshold) & halves_fit
        leaf = active & ~split
        found = np.argwhere(leaf)  # (n, 2): the leaf cells' row and column, in reading order
        if len(found):
            ids = np.full((down, across), -1, np.int32)
            ids[leaf] = sum(map(len, means)) + np.arange(len(found), dtype=np.int32)
            painted = np.repeat(np.repeat(ids, np.diff(rows), axis=0), np.diff(cols), axis=1)
            label = np.where(painted >= 0, painted, label)
            boxes.append(
                np.stack([rows[found[:, 0]], rows[found[:, 0] + 1], cols[found[:, 1]], cols[found[:, 1] + 1]], axis=1)
            )
            means.append(mean[leaf] + centre)
        down, across = 2 * down, 2 * across
        active = split[np.arange(down) // 2][:, np.arange(across) // 2]  # the four halves of each cell that split
    return Leaves(label, np.concatenate(boxes), np.concatenate(means).astype(np.float32))
