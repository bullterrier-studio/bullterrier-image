"""Moving a picture, or one of its channels, by a distance in pixels: whole or fractional, with a rule for the edge."""

import math
from typing import Literal

import numpy as np

Edge = Literal["wrap", "nearest", "reflect", "mirror"]


class BoxedImage:
    """A (height, width) image that is zero outside a small box, shifted many times at a cost that follows the box.

    `shifted(dy, dx)` is the image moved `dy` down and `dx` right (a negative distance moves it up or left) as
    `(rows, cols, block)`: the non-zero part of the result lies inside `block`, which is the result's `[rows, cols]`
    (the bounds are not tight; a box may hold zeros, and an image that is all zero gives an empty block), and the rest
    of the result is exactly 0. A distance that is not a whole number reads between the two lines it lies between,
    linearly along each axis; `edge` says what lies beyond the image: 'wrap' the image repeated, 'nearest' its outermost
    line repeated, 'reflect' mirrored with the edge line repeated once, 'mirror' mirrored about the edge line. `block`
    may be a view of the image's own copy: read it, do not write to it.
    """

    def __init__(self, image: np.ndarray, *, edge: Edge) -> None:
        self._edge = edge
        self._size = image.shape
        self._extent = (_extent(image.any(axis=1)), _extent(image.any(axis=0)))
        (top, bottom), (left, right) = self._extent
        # the box, with a line of zeros on each side: lines just outside it are read in order, so are slices
        self._box = np.zeros((bottom - top + 2, right - left + 2), image.dtype)
        self._box[1:-1, 1:-1] = image[top:bottom, left:right]

    def shifted(self, dy: float, dx: float) -> tuple[slice, slice, np.ndarray]:
        """The shift by `dy` down and `dx` right, as described on the class."""
        rows, row_lines = self._lines_read(self._extent[0], self._size[0], dy)
        cols, col_lines = self._lines_read(self._extent[1], self._size[1], dx)
        block = self._box
        for axis, (near, far, fraction) in enumerate((row_lines, col_lines)):
            taken = _lines(block, near, axis)
            block = taken if fraction == 0 else _blend(taken, _lines(block, far, axis), fraction)
        return rows, cols, block

    def _lines_read(
        self, extent: tuple[int, int], size: int, distance: float
    ) -> tuple[slice, tuple[np.ndarray, np.ndarray, float]]:
        """The output lines along a side of `size` lines, whose box spans `extent`, that read the box, and for them the
        lines of the box (counted from its zero margin) that they read; a line the box does not reach reads the
        margin."""
        low, high = extent
        near, far, fraction = _source_lines(size, distance, self._edge)
        reads = (near >= low) & (near < high)
        if fraction != 0:
            reads |= (far >= low) & (far < high)
        reading = np.flatnonzero(reads)
        span = slice(int(reading[0]), int(reading[-1]) + 1) if reading.size else np.s_[:0]
        # the box's own lines are 1 .. high - low; line 0 and the last are its zero margin, which every other line reads
        near, far = (np.where((line >= low - 1) & (line <= high), line - low + 1, 0)[span] for line in (near, far))
        return span, (near, far, fraction)


def _lines(block: np.ndarray, index: np.ndarray, axis: int) -> np.ndarray:
    """`np.take(block, index, axis)`, as a slice (no copy) where `index` counts up by one."""
    if index.size and (index[1:] - index[:-1] == 1).all():
        return block[index[0] : index[-1] + 1] if axis == 0 else block[:, index[0] : index[-1] + 1]
    return np.take(block, index, axis=axis)


def _extent(occupied: np.ndarray) -> tuple[int, int]:
    """The first line of `occupied` that is set and the one after the last (0, 0 if none is)."""
    first = int(occupied.argmax())
    last = occupied.size - int(occupied[::-1].argmax())
    return (first, last) if occupied.any() else (first, first)


def _shift_axis(image: np.ndarray, distance: float, axis: int, edge: Edge) -> np.ndarray:
    """`image` moved by `distance` px along one axis: output line i is the input at i - distance."""
    near, far, fraction = _source_lines(image.shape[axis], distance, edge)
    if fraction == 0:
        return np.take(image, near, axis=axis)
    return _blend(np.take(image, near, axis=axis), np.take(image, far, axis=axis), fraction)


def _source_lines(size: int, distance: float, edge: Edge) -> tuple[np.ndarray, np.ndarray, float]:
    """For a shift by `distance` along a side of `size` lines: the line each output line reads (`near`), the line before
    it (`far`, read only when the fraction is not 0), and the share of `far` in the output."""
    whole = math.floor(distance)
    lines = np.arange(size)
    # the output at line i lies between the input lines i - whole - 1 and i - whole
    return _line_within(lines - whole, size, edge), _line_within(lines - whole - 1, size, edge), distance - whole


def _blend(near: np.ndarray, far: np.ndarray, fraction: float) -> np.ndarray:
    """`near` and `far` mixed linearly, `fraction` of `far`, in `near`'s dtype."""
    mixed = near * (1 - fraction)
    mixed += far * fraction
    return mixed.astype(near.dtype)


def _line_within(index: np.ndarray, size: int, edge: Edge) -> np.ndarray:
    """The line of a picture of `size` lines that each position of `index` shows, by the rule for beyond its edge."""
    if edge == "wrap":
        return index % size
    if edge == "nearest":
        return np.clip(index, 0, size - 1)
    period = 2 * size if edge == "reflect" else max(2 * size - 2, 1)
    folded = index % period
    return np.where(folded >= size, period - (folded + 1 if edge == "reflect" else folded), folded)
