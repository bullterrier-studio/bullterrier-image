"""Polygons on a pixel grid: which pixels a set of closed outlines covers by the nonzero or the even-odd fill rule, and
how much of each pixel.

Coordinates are px with pixel `(row, column)` covering `[column, column + 1) x [row, row + 1)`. An outline is an
`(n, 2)` array of `(x, y)` corners, closed by itself; several outlines in one call are one shape (a hole is an outline
inside another). Work goes one scanline at a time with no supersampled picture, a few rows at once, so memory stays at
a few rows however big the picture or the outline is.
"""

from typing import Literal

import numpy as np

__all__ = ["FillRule", "coverage", "paint"]

FillRule = Literal["nonzero", "evenodd"]

_ROWS_AT_ONCE = 64  # constant: pixel rows handled together; bounds the memory of the crossings, never changes a result


def _edges(polygons: list[np.ndarray]) -> tuple[np.ndarray, np.ndarray]:
    """The edges of the closed `polygons` that are not horizontal (they cross no scanline): start and end, `(m, 2)`."""
    if not polygons:
        return np.empty((0, 2)), np.empty((0, 2))
    start = np.concatenate(polygons)
    end = np.concatenate([np.roll(polygon, -1, axis=0) for polygon in polygons])
    slanting = start[:, 1] != end[:, 1]
    return start[slanting], end[slanting]


def _spans(
    start: np.ndarray, end: np.ndarray, ys: np.ndarray, rule: FillRule
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """The stretches `[x0, x1)` that are inside along the horizontal scanlines at heights `ys` (ascending): the index
    of the scanline, `x0` and `x1` of each. An edge crosses a scanline when the edge's lower end is at or above it and
    its upper end below it, so a corner exactly on a scanline is counted once."""
    low, high = np.minimum(start[:, 1], end[:, 1]), np.maximum(start[:, 1], end[:, 1])
    first, last = np.searchsorted(ys, low, "left"), np.searchsorted(ys, high, "left")
    crossed = np.flatnonzero(last > first)
    counts = (last - first)[crossed]
    edge = np.repeat(crossed, counts)
    scan = first[edge] + np.arange(len(edge)) - np.repeat(np.cumsum(counts) - counts, counts)
    run = end[edge] - start[edge]
    x = start[edge, 0] + (ys[scan] - start[edge, 1]) * run[:, 0] / run[:, 1]
    order = np.lexsort((x, scan))
    scan, x = scan[order], x[order]
    # Every closed outline crosses a scanline as often one way as the other, so the running sum over all crossings
    # returns to 0 at the end of each scanline: no need to restart it.
    if rule == "nonzero":
        inside = np.cumsum(np.sign(run[order, 1]).astype(np.int64)) != 0
    else:
        inside = np.arange(len(x)) % 2 == 0
    began = np.flatnonzero(inside)
    return scan[began], x[began], x[began + 1]


def _rows(start: np.ndarray, end: np.ndarray, height: int) -> range:
    """The pixel rows from which a scanline could meet the edges: the outlines' vertical extent within the picture."""
    if not len(start):
        return range(0)
    top = int(np.clip(np.floor(min(start[:, 1].min(), end[:, 1].min())), 0, height))
    bottom = int(np.clip(np.ceil(max(start[:, 1].max(), end[:, 1].max())), 0, height))
    return range(top, bottom, _ROWS_AT_ONCE)


def paint(target: np.ndarray, value: object, polygons: list[np.ndarray], *, rule: FillRule) -> None:
    """Set `target[row, column] = value` for every pixel whose centre is inside the `polygons` by the fill `rule`
    ('nonzero' counts the turns round the point, 'evenodd' the crossings; the outlines of one call are one shape).
    A pixel exactly on an edge belongs to the shape when the shape is on its right or below it."""
    height, width = target.shape
    start, end = _edges(polygons)
    for top in _rows(start, end, height):
        bottom = min(top + _ROWS_AT_ONCE, height)
        scan, x0, x1 = _spans(start, end, np.arange(top, bottom) + 0.5, rule)
        # the first and last pixel of a stretch whose centres are inside it
        left = np.clip(np.ceil(x0 - 0.5), 0, width).astype(np.int64)
        right = np.clip(np.ceil(x1 - 0.5), 0, width).astype(np.int64)
        size = (bottom - top) * (width + 1)
        began = np.bincount(scan * (width + 1) + left, minlength=size)
        ended = np.bincount(scan * (width + 1) + right, minlength=size)
        inside = np.cumsum((began - ended).reshape(bottom - top, width + 1), axis=1)[:, :width] > 0
        target[top:bottom][inside] = value


def coverage(polygons: list[np.ndarray], shape: tuple[int, int], rule: FillRule, samples: int) -> np.ndarray:
    """How much of each pixel the `polygons` cover by the fill `rule`: float32 `(height, width)`, 0..1.

    Horizontally the share is exact; vertically each pixel row is measured at `samples` evenly spaced scanlines, so
    the error of a slanting border shrinks as `samples` grows (1 = at the row's middle only)."""
    height, width = shape
    out = np.zeros(shape, np.float32)
    start, end = _edges(polygons)
    for top in _rows(start, end, height):
        bottom = min(top + _ROWS_AT_ONCE, height)
        ys = (np.arange(top * samples, bottom * samples) + 0.5) / samples
        scan, x0, x1 = _spans(start, end, ys, rule)
        row = scan // samples
        size = (bottom - top) * (width + 3)
        steps = np.zeros(size)
        # A stretch from a to b covers pixel i by clip(i + 1 - a, 0, b - a) - clip(i - a, 0, b - a), the difference of
        # two ramps `max(i - a, 0)` and `max(i - b, 0)`; a ramp starting at `a` is a second difference of
        # `ceil(a) - a` at ceil(a) and `1 - (ceil(a) - a)` after it.
        for edge, weight in ((np.clip(x0, 0, width), 1 / samples), (np.clip(x1, 0, width), -1 / samples)):
            cell = np.ceil(edge).astype(np.int64)
            index = row * (width + 3) + cell
            steps += np.bincount(index, weight * (cell - edge), minlength=size)
            steps += np.bincount(index + 1, weight * (1 - (cell - edge)), minlength=size)
        ramps = np.cumsum(np.cumsum(steps.reshape(bottom - top, width + 3), axis=1), axis=1)
        out[top:bottom] = np.diff(ramps[:, : width + 1], axis=1)
    return out
