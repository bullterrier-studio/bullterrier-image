"""Where the sites of a picture's pieces lie: anywhere, or where the picture has detail.

Coordinates as in `ops/voronoi.py`: a site is an `(x, y)` pair in px from the picture's top left corner, and the
pixel in row `i`, column `j` covers `j <= x < j + 1`, `i <= y < i + 1`.
"""

import numpy as np

from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.edges import edge_strength

_UNSMOOTHED = 0.0  # constant: no smoothing, so the strength is the picture's own, pixel by pixel
_TRUNCATE = 1.0  # constant: where a smoothing would be cut off; no smoothing is done, so it changes nothing


def sites(shape: tuple[int, int], count: int, rng: np.random.Generator, *, weights: np.ndarray | None) -> np.ndarray:
    """`count` sites over a picture of `shape` (height, width): a float `(count, 2)` array of `(x, y)`.

    With no `weights` every place in the picture is equally likely. With `weights` (a non-negative `(height, width)`
    array that sums to more than 0) a site falls in a pixel with a probability proportional to the pixel's weight, and
    anywhere inside the pixel. Sites come from `rng`, so the same generator state gives the same sites.
    """
    height, width = shape
    extent = np.array([width, height])  # x then y
    if weights is None:
        return rng.random((count, len(extent))) * extent
    pixel = rng.choice(height * width, size=count, p=weights.ravel() / weights.sum())
    row, column = np.divmod(pixel, width)
    return np.stack([column + rng.random(count), row + rng.random(count)], axis=1)


def detail_weights(rgb: np.ndarray, *, detail: float, edge: Edge) -> np.ndarray:
    """How likely a site is in each pixel of `rgb` (a float32 `(height, width, 3)` picture), `(height, width)` weights
    that sum to 1: a mix of an even share, `1 - detail` of the total (0..1), and a share that follows the picture's
    edges, `detail` of it, in proportion to how steeply the brightness changes at the pixel (the Sobel strength of the
    Rec. 709 luminance, unsmoothed). A picture with no edge at all has only the even share, so it is even whatever
    `detail` is. `edge` is what lies beyond the picture's border for the edge strength (`ops/convolve.Edge`). `rgb` is
    not modified.
    """
    strength = edge_strength(luminance(rgb), sigma=_UNSMOOTHED, truncate=_TRUNCATE, edge=edge)
    even = np.full(strength.shape, 1 / strength.size)
    total = strength.sum()
    if total == 0:
        return even
    return (1 - detail) * even + detail * strength / total
