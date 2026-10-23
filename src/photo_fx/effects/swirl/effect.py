"""The `swirl` effect: the picture twisted about a centre, most at the centre and not at all at the radius."""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect

# scipy's names for what lies beyond the picture's edge, in `map_coordinates`'s own spelling: for interpolation its
# 'wrap' does not repeat the picture but 'grid-wrap' does
_BEYOND_THE_EDGE = {"reflect": "reflect", "mirror": "mirror", "nearest": "nearest", "wrap": "grid-wrap"}
# constant: scipy's spline order for each documented interpolation choice (0 nearest pixel, 1 linear, 3 cubic)
_SPLINE_ORDER = {"nearest": 0, "bilinear": 1, "bicubic": 3}


class SwirlParams(EffectParams):
    """A swirl: the picture turned about a centre by an angle that falls smoothly from `angle` at the centre to nothing
    at `radius`."""

    angle: float = param(
        90.0,
        unit="degrees",
        min=-720,
        max=720,
        doc="How far the picture is turned at the centre, in degrees: positive turns clockwise, negative "
        "counter-clockwise, 360 is a whole turn. The turn falls off with the square of the distance left to the "
        "radius, so the picture is twisted into a spiral and unturned at the radius. Larger is a tighter spiral; 0 "
        "leaves the picture unchanged.",
    )
    radius: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=2,
        doc="How far from the centre the swirl reaches, as a fraction of the shorter side of the picture, measured "
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
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the swirl sees beyond the picture's edge (a swirl near the border turns pixels in from outside): "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="distortion", params=SwirlParams)
def swirl(rgb: np.ndarray, params: SwirlParams) -> np.ndarray:
    """Twist the picture about the centre `x`, `y` by `angle` degrees there, falling to nothing at `radius`.

    Each pixel at distance d from the centre, d inside the radius, shows the picture at the point turned about the
    centre by angle x (1 - d / radius)^2, clockwise for a positive angle, read by `interpolation`. Pixels at the radius
    or beyond, and the centre itself, keep their place, and the result never leaves 0..1.
    """
    height, width = rgb.shape[:2]
    reach = params.radius * (min(height, width) - 1)  # in pixels
    if params.angle == 0 or reach == 0:
        return rgb.copy()
    dr = np.arange(height, dtype=np.float64)[:, None] - params.y * (height - 1)  # offsets from the centre, in pixels
    dc = np.arange(width, dtype=np.float64)[None, :] - params.x * (width - 1)
    distance = np.hypot(dr, dc)
    turn = np.radians(params.angle) * np.clip(1 - distance / reach, 0, 1) ** 2
    rows = params.y * (height - 1) + dr * np.cos(turn) - dc * np.sin(turn)
    cols = params.x * (width - 1) + dr * np.sin(turn) + dc * np.cos(turn)
    where = np.stack([rows, cols])
    out = np.stack(
        [
            ndimage.map_coordinates(
                np.ascontiguousarray(rgb[..., channel]),
                where,
                order=_SPLINE_ORDER[params.interpolation],
                mode=_BEYOND_THE_EDGE[params.edge],
            )
            for channel in range(rgb.shape[2])
        ],
        axis=-1,
    )
    return np.clip(out, 0, 1).astype(np.float32)
