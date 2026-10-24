"""The `pinch_bulge` effect: the picture pushed out (a bulge) or in (a pinch) about a centre."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.warp import Centre, remap


class PinchBulgeParams(EffectParams):
    """A pinch or bulge: the picture about a centre magnified (bulge) or shrunk (pinch) most at the centre and not at
    all at `radius`."""

    amount: float = param(
        0.5,
        unit="fraction",
        min=-1,
        max=1,
        doc="How far the middle is pushed. Positive is a bulge: the picture at the centre is magnified, as seen "
        "through a lens or a bubble, ever more toward the centre and without limit at 1 (the pixels around the "
        "centre all show the centre pixel). Negative is a pinch: the picture at the centre is shrunk, as if drawn "
        "into a point, down to half size at -1. The push fades smoothly to nothing at the radius, so no pixel is "
        "folded over; 0 leaves the picture unchanged.",
    )
    radius: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=2,
        doc="How far from the centre the effect reaches, as a fraction of the shorter side of the picture, measured "
        "from the first pixel to the last (0.5 = a disc as wide as half of it, 2 = a disc that covers the whole "
        "picture and more). Pixels from this far out are untouched; 0 leaves the picture unchanged.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = "
        "the middle, 1 = the last.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but a bulge turns blocky), 'bilinear' blends the four pixels around it "
        "(smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little "
        "slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a pinch sees beyond the picture's edge (it reads from farther out than it shows): 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=PinchBulgeParams)
def pinch_bulge(rgb: np.ndarray, params: PinchBulgeParams) -> np.ndarray:
    """Push the picture out (`amount` above 0, a bulge) or in (below 0, a pinch) about the centre `x`, `y`.

    Each pixel at distance d from the centre, d inside the radius, shows the picture on the same ray from the centre
    at distance d x (1 - amount x (1 - d / radius)^2), read by `interpolation`: nearer the centre for a bulge, so
    the middle is magnified, and farther out for a pinch. Pixels at the radius or beyond, and the centre itself, keep
    their place, no pixel is folded over, and the result never leaves 0..1.
    """
    height, width = rgb.shape[:2]
    reach = params.radius * (min(height, width) - 1)  # in pixels
    if params.amount == 0 or reach == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    scale = 1 - params.amount * np.clip(1 - centre.distance / reach, 0, 1) ** 2
    rows, cols = centre.sources(scale=scale)
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
