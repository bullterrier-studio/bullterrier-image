"""Voronoi cells and Delaunay triangles over a picture, for mosaic, stained-glass, low-poly and stipple effects.

Coordinates: a picture of `shape = (h, w)` is the rectangle `0 <= x < w`, `0 <= y < h`, and the pixel in row `i`,
column `j` has its centre at `(x, y) = (j + 0.5, i + 0.5)`. Points are `(n, 2)` float32 arrays of `(x, y)`; per-pixel
cell labels are `(h, w)` int32 arrays holding the index of the nearest point. Every function is pure and deterministic,
and never modifies its arguments. Invalid arguments raise `ValueError`.
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
    return _nearest(points, _sample_centres(h, w, h, w)).reshape(h, w)


def cell_means(image: np.ndarray, labels: np.ndarray, count: int) -> np.ndarray:
    """Return the mean colour of each cell.

    Args:
        image: `(h, w)` or `(h, w, c)` array of any numeric values (typically float32 in 0..1).
        labels: `(h, w)` integer cell labels, as from `cell_labels`; every label must be in `[0, count)`.
        count: number of cells.

    Returns:
        `(count, c)` float32 array (`c = 1` for a 2-D image); a cell that owns no pixel gets zeros.
    """
    if labels.size and (labels.min() < 0 or labels.max() >= count):
        raise ValueError(f"labels must lie in [0, {count}), got [{labels.min()}, {labels.max()}]")
    flat = labels.ravel()
    channels = image.reshape(flat.size, -1)
    sizes = np.bincount(flat, minlength=count)
    # float64 sums keep the mean of identical values exactly equal to that value
    sums = np.stack(
        [np.bincount(flat, weights=channels[:, c], minlength=count) for c in range(channels.shape[1])], axis=1
    )
    return (sums / np.maximum(sizes, 1)[:, None]).astype(np.float32)


def cell_borders(labels: np.ndarray) -> np.ndarray:
    """Mark the pixels that sit on a boundary between two cells.

    Args:
        labels: `(h, w)` integer cell labels, as from `cell_labels`.

    Returns:
        `(h, w)` bool array, True where the pixel's right or lower neighbour belongs to a different cell. A boundary is
        therefore one pixel thick and lies on the upper-left side of the change; the last row and column have no
        neighbour beyond the picture and are flagged only by their other neighbour.
    """
    borders = np.zeros(labels.shape, dtype=bool)
    borders[:, :-1] |= labels[:, :-1] != labels[:, 1:]
    borders[:-1, :] |= labels[:-1, :] != labels[1:, :]
    return borders


# Lloyd relaxation only needs centroids good to a fraction of a cell, so large pictures are sampled on a coarser grid.
_RELAX_MAX_SAMPLES = 256


def _sample_centres(h: int, w: int, rows: int, cols: int) -> np.ndarray:
    """`(rows * cols, 2)` float64 (x, y) centres of a `rows x cols` grid of equal tiles covering the h x w picture."""
    ys = (np.arange(rows) + 0.5) * (h / rows)
    xs = (np.arange(cols) + 0.5) * (w / cols)
    return np.stack(np.meshgrid(xs, ys), axis=-1).reshape(-1, 2)


def _nearest(points: np.ndarray, samples: np.ndarray) -> np.ndarray:
    """int32 index of the point nearest each sample position."""
    _, nearest = cKDTree(points.astype(np.float64)).query(samples, workers=-1)
    return nearest.astype(np.int32)


def relax(points: np.ndarray, shape: tuple[int, int], iterations: int = 3) -> np.ndarray:
    """Spread points evenly with Lloyd relaxation: repeatedly move each point to the centroid of its Voronoi cell.

    Args:
        points: `(n, 2)` array of `(x, y)` in pixels, inside the picture.
        shape: picture size `(h, w)` in pixels.
        iterations: number of relaxation rounds, 0 or more; 0 returns a copy of `points`. Three rounds give visibly
            even spacing; more rounds approach a regular hexagonal pattern.

    Returns:
        New `(n, 2)` float32 array, same order as `points`, every point still inside the picture. A point whose cell
        holds no pixel (for example a duplicate) stays where it was.
    """
    if iterations < 0:
        raise ValueError(f"iterations must be 0 or more, got {iterations}")
    h, w = shape
    step = max(1, -(-max(h, w) // _RELAX_MAX_SAMPLES))
    samples = _sample_centres(h, w, -(-h // step), -(-w // step))
    current = points.astype(np.float32)
    for _ in range(iterations):
        labels = _nearest(current, samples)
        sizes = np.bincount(labels, minlength=len(current))
        sums = np.stack(
            [np.bincount(labels, weights=samples[:, axis], minlength=len(current)) for axis in (0, 1)], axis=1
        )
        occupied = sizes > 0
        current = current.copy()
        current[occupied] = sums[occupied] / sizes[occupied, None]
    return current
