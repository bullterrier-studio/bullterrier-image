"""Voronoi cells and Delaunay triangles over a picture, for mosaic, stained-glass, low-poly and stipple effects.

Coordinates: a picture of `shape = (h, w)` is the rectangle `0 <= x < w`, `0 <= y < h`, and the pixel in row `i`, column `j`
has its centre at `(x, y) = (j + 0.5, i + 0.5)`. Points are `(n, 2)` float32 arrays of `(x, y)`; per-pixel cell labels are
`(h, w)` int32 arrays holding the index of the nearest point. Every function is pure and deterministic, and never modifies
its arguments. Invalid arguments raise `ValueError`.
"""

import numpy as np
from scipy.spatial import cKDTree


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


def cell_labels(shape: tuple[int, int], points: np.ndarray) -> np.ndarray:
    """Label every pixel with the index of its nearest point (the pixel's Voronoi cell).

    Args:
        shape: picture size `(h, w)` in pixels.
        points: `(n, 2)` array of `(x, y)` in pixels, `n >= 1`; they may lie anywhere, inside the picture or not.

    Returns:
        `(h, w)` int32 array; each pixel holds exactly one label in `[0, n)`, the row of `points` nearest its centre.
    """
    if len(points) == 0:
        raise ValueError("cell_labels needs at least one point")
    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    centres = np.stack([xs.ravel() + 0.5, ys.ravel() + 0.5], axis=1)
    _, nearest = cKDTree(points.astype(np.float64)).query(centres, workers=-1)
    return nearest.reshape(h, w).astype(np.int32)
