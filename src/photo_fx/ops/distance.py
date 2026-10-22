"""How far each cell (or pixel) of a shape is from the shape's edge, and which way is out.

For effects that act on a subject's outline or grow things away from it: outlines, shadows, rim light, auras, or pieces
that detach from the edge first. Works on any 2-D bool mask: a full-resolution matte or a coarse grid of pieces.
"""

from dataclasses import dataclass

import numpy as np
from scipy import ndimage


@dataclass(frozen=True)
class EdgeField:
    """Per-element distance from the edge of a shape, and the direction out of it. All arrays have the shape's shape.

    distance: px from each element to the nearest edge of the shape; 0 outside it.
    normal_x, normal_y: unit vector pointing OUT of the shape (x to the right, y pointing down). Points up (0, -1)
        where the distance is flat, because up is where things drift by default.
    """

    distance: np.ndarray
    normal_x: np.ndarray
    normal_y: np.ndarray


def edge_distance(
    shape: np.ndarray,
    sampling: tuple[float, float] = (1.0, 1.0),  # constant: one px per element, a unit grid
    *,
    open_bottom: bool = True,
) -> EdgeField:
    """Distance to the nearest edge, and the outward direction, for every element of a 2-D bool mask.

    `shape` is the mask (True = inside the shape; ValueError unless it is a non-empty 2-D bool array). Its edges are the
    boundary between True and False, plus the left, right and top borders of the array, so a shape touching those
    borders is edged there too. The bottom border is an edge only when `open_bottom` is False; by default the shape
    is taken to carry on past the bottom, as a portrait does when it is cropped at the chest.

    `sampling` = (row height, column width) is the size of one element in px (both > 0, else ValueError), so a grid of
    tall characters gives distances in px of the picture, not in cells. Distances are in the same unit.

    The direction is the negative gradient of the distance, lightly smoothed (1 element) so that it varies gently
    across a ridge instead of flipping.
    """
    if not isinstance(shape, np.ndarray) or shape.dtype != bool or shape.ndim != 2 or shape.size == 0:
        raise ValueError("shape must be a non-empty 2-D bool array")
    if len(sampling) != 2 or not all(np.isfinite(s) and s > 0 for s in sampling):
        raise ValueError(f"sampling must be two positive numbers (row height, column width), got {sampling!r}")
    row_height, col_width = sampling

    # A border is an edge by putting a row/column of "outside" next to it. Leaving the bottom unpadded is what makes it
    # open: nothing outside the array can be nearer than what is already in it.
    grid = np.pad(shape, ((1, 0 if open_bottom else 1), (1, 1)), constant_values=False)
    distance = ndimage.distance_transform_edt(grid, sampling=sampling)[1 : 1 + shape.shape[0], 1:-1]

    # Smoothing is over 1 element (not px), the same for every sampling. A gradient under 1e-6 per px is float noise or
    # beyond the reach of the smoothing (4 elements from the nearest inside element): there is no direction there.
    smooth = ndimage.gaussian_filter(distance, 1.0)
    grad_y = np.gradient(smooth, row_height, axis=0) if shape.shape[0] > 1 else np.zeros_like(smooth)
    grad_x = np.gradient(smooth, col_width, axis=1) if shape.shape[1] > 1 else np.zeros_like(smooth)
    length = np.hypot(grad_x, grad_y)
    flat = length < 1e-6  # constant: float noise, or beyond the smoothing's reach (see above)
    safe = np.where(flat, 1.0, length)
    return EdgeField(
        distance=distance,
        normal_x=np.where(flat, 0.0, -grad_x / safe),
        normal_y=np.where(flat, -1.0, -grad_y / safe),
    )
