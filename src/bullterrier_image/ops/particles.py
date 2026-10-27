"""Drawing particles: soft discs and streaks, each at its own place, size and brightness, onto one layer of light."""

import numpy as np


def draw_segments(
    shape: tuple[int, int],
    start: np.ndarray,
    end: np.ndarray,
    radius: float | np.ndarray,
    weight: float | np.ndarray,
    feather: float | np.ndarray = 1.0,  # constant: one pixel, the finest edge a pixel grid can draw
) -> np.ndarray:
    """The (height, width) float32 layer of light that segments from `start` to `end` leave, brightest where they cross.

    `start` and `end` are (n, 2) arrays of x, y positions in px, a pixel's centre at whole numbers; every `start`
    lies inside the picture, an `end` may lie beyond it. A segment whose ends are equal is a disc. `radius` (the
    half-width of the segment, px), `weight` (its brightness along its centre line, 0..1) and `feather` (the width of
    its soft edge, px) are each one number for every segment or an array of n. A feather under 1 is taken as 1: a
    narrower edge is aliasing, the pixel grid's own limit; without one it is 1, a pixel of antialiasing.

    A pixel's value is `weight` times how far the segment covers it: 1 within `radius` - `feather` / 2 of the segment
    (its centre line, or the point of a disc), 0 beyond `radius` + `feather` / 2, and falling linearly between, so
    exactly 0.5 at `radius`; a feather of 1 is one pixel of antialiasing. Where segments overlap the brighter one
    shows: light does not add up. Parts beyond the picture's edge are cut off.
    """
    height, width = shape
    layer = np.zeros(shape, np.float32)
    count = len(start)
    radii, weights, feathers = (np.broadcast_to(value, count) for value in (radius, weight, feather))
    feathers = np.maximum(feathers, 1.0)
    # each segment's window: its bounding box grown by its reach and a pixel, cut to the picture; outside it covers 0
    margin = (radii + feathers / 2 + 1)[:, None]
    left, top = np.maximum(np.minimum(start, end) - margin, 0).astype(np.intp).T
    right, bottom = np.minimum((np.maximum(start, end) + margin).astype(np.intp) + 1, (width, height)).T
    # Windows of similar area go together, padded to the largest of their batch (the padding covers 0), and a batch
    # is about `budget` pixels so that its temporaries stay in the cache.
    # constant: 2**18 values of 8 bytes are 2 MB, a few of which fit a CPU cache
    budget = 2**18
    order = np.argsort((bottom - top) * (right - left))
    tall, wide = (bottom - top)[order], (right - left)[order]
    first = 0  # constant: the first of the sorted windows
    while first < count:
        last = min(first + max(budget // (tall[first] * wide[first]), 1), count)
        while last - first > 1 and (last - first) * tall[first:last].max() * wide[first:last].max() > budget:
            last = first + (last - first) // 2
        rows = top[order[first:last], None, None] + np.arange(tall[first:last].max())[:, None]
        cols = left[order[first:last], None, None] + np.arange(wide[first:last].max())
        x0, y0, x1, y1, reach, light, edge = (
            value[order[first:last], None, None] for value in (*start.T, *end.T, radii, weights, feathers)
        )
        first = last
        along_x, along_y = x1 - x0, y1 - y0
        squared = along_x**2 + along_y**2
        # how far along the segment (0..1) the nearest point to each pixel is
        # constant: a point has no length to divide by, and its nearest point is its own
        nearest = np.clip(((cols - x0) * along_x + (rows - y0) * along_y) / np.where(squared > 0, squared, 1), 0, 1)
        distance = np.hypot(cols - x0 - nearest * along_x, rows - y0 - nearest * along_y)
        cover = light * np.clip((reach + 0.5 * edge - distance) / edge, 0.0, 1.0)
        drawn = (rows < height) & (cols < width) & (cover > 0)
        np.maximum.at(layer.ravel(), (rows * width + cols)[drawn], cover[drawn].astype(np.float32))
    return layer
