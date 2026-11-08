"""Warping a picture: reading it at float coordinates."""

from typing import Literal

import numpy as np
from scipy import ndimage

# The one table of the edge rules for scipy's interpolating functions (`map_coordinates`, `affine_transform`), in their
# own spelling: for interpolation scipy's 'wrap' does not repeat the picture but 'grid-wrap' does.
SCIPY_EDGE = {"reflect": "reflect", "mirror": "mirror", "nearest": "nearest", "wrap": "grid-wrap"}
# The one table of the spline orders.
# constant: scipy's spline order for each documented interpolation choice (0 nearest pixel, 1 linear, 3 cubic)
SPLINE_ORDER = {"nearest": 0, "bilinear": 1, "bicubic": 3}
# constant: columns copied from the far side when columns wrap; a cubic spline's end effect falls by a factor of
# 0.27 per pixel (2 - sqrt 3), so 16 leave under 1e-8
_SEAM = 16


def remap(
    rgb: np.ndarray,
    rows: np.ndarray,
    cols: np.ndarray,
    *,
    interpolation: Literal["nearest", "bilinear", "bicubic"],
    edge: Literal["reflect", "mirror", "nearest", "wrap"],
    wrap_columns: bool = False,
) -> np.ndarray:
    """The picture `rgb` read at the float positions (`rows`, `cols`), each colour channel on its own.

    `rows` and `cols` have the same shape, any shape, and say where to read: a pixel's centre is at whole numbers,
    row 0 and column 0 the first. The result has that shape plus the channels, float32, in 0..1 (a cubic spline can
    overshoot; it is clipped). `interpolation` is how a position between pixels is read: 'nearest' the closest pixel,
    'bilinear' the blend of the four around it, 'bicubic' a cubic spline through the neighbours. `edge` is what lies
    beyond the picture: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel),
    'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). Reading every pixel where it is returns the
    picture; `rgb` is never modified.

    With `wrap_columns` the picture is periodic across its columns, as an angle is: the column after the last is the
    first, whatever the `edge` rule, which then only applies to the rows (a position at any column, however far,
    is read modulo the width).
    """
    if wrap_columns:
        width = rgb.shape[1]
        rgb = rgb[:, np.arange(-_SEAM, width + _SEAM) % width]  # the picture with its far side copied on each side
        cols = cols % width + _SEAM
    where = np.stack([rows, cols])
    out = np.stack(
        [
            ndimage.map_coordinates(
                np.ascontiguousarray(rgb[..., channel]),
                where,
                order=SPLINE_ORDER[interpolation],
                mode=SCIPY_EDGE[edge],
            )
            for channel in range(rgb.shape[2])
        ],
        axis=-1,
    )
    return np.clip(out, 0, 1)
