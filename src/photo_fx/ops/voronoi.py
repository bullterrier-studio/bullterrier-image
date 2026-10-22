"""Voronoi cells and Delaunay triangles over a picture, for mosaic, stained-glass, low-poly and stipple effects.

Coordinates: a picture of `shape = (h, w)` is the rectangle `0 <= x < w`, `0 <= y < h`, and the pixel in row `i`, column `j`
has its centre at `(x, y) = (j + 0.5, i + 0.5)`. Points are `(n, 2)` float32 arrays of `(x, y)`; per-pixel cell labels are
`(h, w)` int32 arrays holding the index of the nearest point. Every function is pure and deterministic, and never modifies
its arguments. Invalid arguments raise `ValueError`.
"""

import numpy as np


def random_points(shape: tuple[int, int], count: int, seed: int) -> np.ndarray:
    """Return `count` points drawn uniformly inside the picture.

    Args:
        shape: picture size `(h, w)` in pixels.
        count: number of points, 0 or more.
        seed: random seed; the same seed always gives the same points.

    Returns:
        `(count, 2)` float32 array of `(x, y)` with `0 <= x < w` and `0 <= y < h`.
    """
    if count < 0:
        raise ValueError(f"count must be 0 or more, got {count}")
    h, w = shape
    unit = np.random.default_rng(seed).random((count, 2))
    points = (unit * (w, h)).astype(np.float32)
    # Rounding a value just below w to float32 can land exactly on w, which is outside the picture.
    limit = np.array([np.nextafter(np.float32(w), np.float32(0)), np.nextafter(np.float32(h), np.float32(0))])
    return np.minimum(points, limit)
