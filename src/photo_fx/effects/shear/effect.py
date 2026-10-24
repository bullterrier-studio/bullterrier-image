"""The `shear` effect: the picture skewed about its centre, slid sideways row by row and up or down column by column."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.warp import Centre, remap


class ShearParams(EffectParams):
    """A shear: the picture skewed about its centre by a slope along each axis."""

    x: float = param(
        0.2,
        unit="ratio",
        min=-2,
        max=2,
        doc="The horizontal skew as a slope, in columns per row: each row slides sideways by this many pixels for "
        "every pixel it lies below the centre (and the other way above it), so a vertical line leans into a line of "
        "this slope. 0 = no skew, 0.2 = a lean of about 11 degrees, 1 = 45 degrees, positive slides the lower half "
        "right and the upper half left, negative the other way.",
    )
    y: float = param(
        0.0,
        unit="ratio",
        min=-2,
        max=2,
        doc="The vertical skew as a slope, in rows per column: each column slides down by this many pixels for every "
        "pixel it lies right of the centre (and up on the left of it), so a horizontal line tilts into a line of this "
        "slope. 0 = no skew, positive tilts the right side down, negative the right side up. With both skews, `x` is "
        "applied first, then `y`, so the picture's area never changes.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What fills the corners the skew uncovers, where it slides the picture away and shows what lies beyond its "
        "edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the "
        "edge pixel repeated: streaks), 'wrap' (the opposite edge).",
    )


@effect(category="distortion", params=ShearParams)
def shear(rgb: np.ndarray, params: ShearParams) -> np.ndarray:
    """Skew the picture about its centre: slide rows sideways by `x` px per row, then columns down by `y` px per column.

    A pixel d rows below the centre moves x x d columns right, and then a pixel c columns right of the centre (counted
    after that move) moves y x c rows down; the centre pixel stays, and so does the centre row under `x` alone and the
    centre column under `y` alone. Points between pixels are read by `interpolation`.
    """
    height, width = rgb.shape[:2]
    dr, dc = Centre.middle(height, width).offsets()
    rows = np.arange(height)[:, None] - params.y * dc  # undo the later skew: the row the pixel was in before it
    cols = np.arange(width)[None, :] - params.x * (dr - params.y * dc)  # then the earlier one, from the row it left
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
