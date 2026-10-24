"""The `radial_blur` effect: the picture smeared around a centre (spin) or away from it (zoom)."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.radial import accumulate


class RadialBlurParams(EffectParams):
    """A radial blur: every pixel becomes the average of the picture along an arc around the centre, or a ray through
    it."""

    mode: Literal["spin", "zoom"] = param(
        "zoom",
        doc="The direction of the smear: 'spin' averages along the circle around the centre, as a rotating camera or "
        "wheel would blur; 'zoom' averages along the ray through the centre, as zooming the lens during the exposure "
        "would. The centre itself stays sharp in both.",
    )
    amount: float = param(
        0.1,
        unit="fraction",
        min=0,
        max=1,
        doc="How far each pixel is smeared. 'spin': the arc, as a fraction of a full turn (0.1 = 36 degrees, 1 = a "
        "whole circle), centred on the pixel. 'zoom': the range of scales, as a fraction of the pixel's distance from "
        "the centre (0.1 = from 5% closer to 5% farther, 1 = from half to one and a half times the distance). Larger "
        "is a longer smear and stronger at the edges; 0 leaves the picture unchanged.",
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
    samples: int = param(
        8,
        unit="count",
        min=1,
        max=256,
        doc="How many points along each arc or ray are averaged, spread evenly over its length and read from the "
        "picture by the chosen interpolation: more is smoother and slower (the time grows with it), and a smear of "
        "more than a few pixels per sample shows as separate ghost copies. With 1 the picture is unchanged.",
        advanced=True,
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but steps and ghosts show in smooth areas), 'bilinear' blends the four pixels "
        "around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest "
        "and most exact on smooth gradients, a little slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the smear sees beyond the picture's edge (arcs and rays near the corners leave it): 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=RadialBlurParams)
def radial_blur(rgb: np.ndarray, params: RadialBlurParams) -> np.ndarray:
    """Smear the picture around a centre (`mode` 'spin') or away from it ('zoom'), by `amount`.

    Every pixel becomes the average of `samples` points spread evenly along the arc or ray through it (centred on the
    pixel), each read by `interpolation`, each colour channel on its own. The centre stays sharp, flat areas
    stay flat and the result never leaves the picture's own range.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    centre = (params.y * (height - 1), params.x * (width - 1))  # (row, column) of the centre, in pixels
    along = (np.arange(params.samples) + 0.5) / params.samples - 0.5  # evenly spread over -1/2 .. 1/2, symmetric
    # 'zoom': the picture's scales, 1 + this, about the centre; 'spin': its angles in radians (a turn is 2 pi)
    offsets = params.amount * along if params.mode == "zoom" else 2 * np.pi * params.amount * along
    summed = accumulate(
        rgb,
        centre=centre,
        kind=params.mode,
        offsets=offsets,
        weights=[1.0] * params.samples,
        interpolation=params.interpolation,
        edge=params.edge,
    )
    return np.clip(summed / params.samples, rgb.min(), rgb.max()).astype(np.float32)
