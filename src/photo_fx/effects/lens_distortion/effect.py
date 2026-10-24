"""The `lens_distortion` effect: barrel or pincushion distortion of a lens, with the picture scaled back to fit."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.warp import Centre, remap


class LensDistortionParams(EffectParams):
    """Lens distortion: the picture bent about the lens axis by the radial polynomial of the Brown-Conrady model, so
    straight lines bow outward (barrel) or inward (pincushion)."""

    distortion: float = param(
        0.2,
        unit="ratio",
        min=-0.3,
        max=1,
        doc="The lens's radial distortion coefficient (the k1 of the Brown-Conrady model), with the distance from the "
        "axis measured in units of the farthest corner's. Positive is barrel distortion: straight lines bow "
        "outward and the picture looks wrapped around a ball. Negative is pincushion: lines bow inward and the "
        "corners are pulled out. The size is how much farther from the axis than its own place each pixel's source "
        "lies at the farthest corner: 0.2 is 20% farther. -0.3 is the most pincushion that does not fold the "
        "picture over, 1 the strongest barrel; 0 leaves the picture unchanged.",
    )
    fit: Literal["frame", "centre"] = param(
        "frame",
        doc="How the picture is scaled back after the bend: 'frame' keeps the farthest corner in its corner, so the "
        "picture fills the frame (a barrel is zoomed in and loses its sides, and nothing beyond the picture is "
        "shown; a pincushion is zoomed out toward the middle and pulls in what lies beyond the middle of each edge, "
        "which the edge rule fills); 'centre' keeps the scale at the lens axis, so a "
        "barrel shrinks toward the middle, and the edge rule fills the corners it exposes.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The lens axis's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The lens axis's vertical position as a fraction of the way down the picture: 0 = the first pixel row, "
        "0.5 = the middle, 1 = the last.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but lines turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What shows where the bend reads from beyond the picture (with fit 'centre', or a pincushion): "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="distortion", params=LensDistortionParams)
def lens_distortion(rgb: np.ndarray, params: LensDistortionParams) -> np.ndarray:
    """Bend the picture as a lens with radial distortion `distortion` about the axis `x`, `y` does.

    Each pixel at distance d from the axis shows the picture on the same ray at d x (1 + distortion x (d / R)^2), R the
    distance of the farthest corner (the Brown-Conrady radial term, k1 only), divided by 1 + distortion for `fit`
    'frame' so that the corner stays put (a barrel then reads only inside the picture). The axis never moves, and the
    result never leaves 0..1.
    """
    height, width = rgb.shape[:2]
    # the farthest corner, in pixels: where the radius is 1
    farthest = np.hypot(max(params.y, 1 - params.y) * (height - 1), max(params.x, 1 - params.x) * (width - 1))
    if params.distortion == 0 or farthest == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    scale = 1 + params.distortion * (centre.distance / farthest) ** 2
    if params.fit == "frame":
        scale = scale / (1 + params.distortion)
    rows, cols = centre.sources(scale=scale)
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
