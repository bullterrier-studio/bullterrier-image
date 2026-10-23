"""Grouping points into clusters: k-means centres and the nearest-centre assignment.

For effects that reduce a picture to a few colours. Points are rows of an `(n, d)` array in whatever space the caller
measures distance in (Euclidean distance is used, so give it a perceptual colour space); nothing here knows about
colours or pictures.

Promises:
- Deterministic: the same points, arguments and `seed` give the same centres. All randomness comes from
  `numpy.random.default_rng(seed)`.
- Inputs are never modified; centres come back as float32.
- Memory stays bounded however many points there are: distances are computed in blocks of rows.
"""

from typing import Literal

import numpy as np

_ROWS_PER_BLOCK = 8192  # constant: rows compared at once, which caps the distance matrix near 16 MB at 256 centres


def nearest(points: np.ndarray, centres: np.ndarray) -> np.ndarray:
    """Index of the closest centre (Euclidean) for every row of `points`: an int array of length `len(points)`.

    A point equally close to several centres goes to the one listed first.
    """
    centres64 = np.asarray(centres, dtype=np.float64)
    half_square = (centres64**2).sum(axis=1) / 2
    labels = np.empty(len(points), dtype=np.intp)
    for start in range(0, len(points), _ROWS_PER_BLOCK):
        block = np.asarray(points[start : start + _ROWS_PER_BLOCK], dtype=np.float64)
        # |p - c|^2 = |p|^2 - 2 p.c + |c|^2, and |p|^2 is the same for every centre, so it cannot change the winner
        labels[start : start + len(block)] = np.argmin(half_square - block @ centres64.T, axis=1)
    return labels


def kmeans(
    points: np.ndarray,
    k: int,
    *,
    init: Literal["k-means++", "random"],
    iterations: int,
    tolerance: float,
    seed: int,
) -> np.ndarray:
    """Centres of `k` clusters of `points` by Lloyd's algorithm: a float32 array `(k, d)`.

    Each round assigns every point to its nearest centre, then moves each centre to the mean of its points. A centre
    left with no points jumps to the point farthest from its own centre, so no centre is wasted. There are fewer than
    `k` centres only when there are fewer than `k` points (one per point).

    init: how the first centres are chosen. "random" takes `k` distinct points; "k-means++" (Arthur and Vassilvitskii,
        2007) takes them one at a time, each with probability proportional to its squared distance from the centres
        chosen so far, which spreads them out and usually ends in a better clustering.
    iterations: the most rounds to run (at least 1).
    tolerance: stop early once no centre moved farther than this in a round (in the units of `points`); 0 runs until
        nothing moves at all.
    seed: seeds the choice of the first centres, and nothing else is random.
    """
    pts = np.asarray(points, dtype=np.float64)
    count, dims = pts.shape
    k = min(k, count)
    rng = np.random.default_rng(seed)
    centres = _first_centres(pts, k, init, rng)
    for _ in range(iterations):
        labels = nearest(pts, centres)
        members = np.bincount(labels, minlength=k)
        sums = np.stack([np.bincount(labels, weights=pts[:, axis], minlength=k) for axis in range(dims)], axis=1)
        moved = centres.copy()
        filled = members > 0
        moved[filled] = sums[filled] / members[filled, None]
        empty = np.flatnonzero(~filled)
        if empty.size:
            outliers = np.argsort(((pts - moved[labels]) ** 2).sum(axis=1), kind="stable")[::-1]
            moved[empty] = pts[outliers[: empty.size]]
        shift = float(np.sqrt(((moved - centres) ** 2).sum(axis=1)).max())
        centres = moved
        if shift <= tolerance:
            break
    return centres.astype(np.float32)


def _first_centres(pts: np.ndarray, k: int, init: str, rng: np.random.Generator) -> np.ndarray:
    """The `k` starting centres, as float64 rows copied from `pts`."""
    if init == "random":
        return pts[rng.choice(len(pts), size=k, replace=False)]
    chosen = [int(rng.integers(len(pts)))]
    gap = ((pts - pts[chosen[0]]) ** 2).sum(axis=1)  # squared distance to the nearest centre chosen so far
    for _ in range(1, k):
        total = gap.sum()
        pick = int(rng.choice(len(pts), p=gap / total)) if total > 0 else int(rng.integers(len(pts)))
        chosen.append(pick)
        gap = np.minimum(gap, ((pts - pts[pick]) ** 2).sum(axis=1))
    return pts[chosen]
