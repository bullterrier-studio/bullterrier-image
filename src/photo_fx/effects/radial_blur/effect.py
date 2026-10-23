"""The `radial_blur` effect: the picture smeared around a centre (spin) or away from it (zoom)."""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect

# scipy's names for what lies beyond the picture's edge, as `photo_fx.ops.blur` uses them, in `map_coordinates`'s own
# spelling: for interpolation its 'wrap' does not repeat the picture but 'grid-wrap' does
_BEYOND_THE_EDGE = {"reflect": "reflect", "mirror": "mirror", "nearest": "nearest", "wrap": "grid-wrap"}


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
        "picture by bilinear interpolation: more is smoother and slower (the time grows with it), and a smear of "
        "more than a few pixels per sample shows as separate ghost copies. With 1 the picture is unchanged.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the smear sees beyond the picture's edge (arcs and rays near the corners leave it): 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
    )


@effect(category="light", params=RadialBlurParams)
def radial_blur(rgb: np.ndarray, params: RadialBlurParams) -> np.ndarray:
    """Smear the picture around a centre (`mode` 'spin') or away from it ('zoom'), by `amount`.

    Every pixel becomes the average of `samples` points spread evenly along the arc or ray through it (centred on the
    pixel), each read by bilinear interpolation, each colour channel on its own. The centre stays sharp, flat areas
    stay flat and the result never leaves the picture's own range.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    centre = np.array([params.y * (height - 1), params.x * (width - 1)])  # (row, column) of the centre, in pixels
    planes = [np.ascontiguousarray(rgb[..., channel]) for channel in range(rgb.shape[2])]
    totals = [np.zeros_like(plane) for plane in planes]
    for k in range(params.samples):
        along = (k + 0.5) / params.samples - 0.5  # evenly spread over -1/2 .. 1/2, symmetric about the pixel itself
        if params.mode == "zoom":
            # the sample of a pixel p is centre + scale (p - centre): the picture scaled about the centre
            matrix = np.full(2, 1.0 + params.amount * along)  # a diagonal matrix: scipy's fast path
            offset = centre - matrix * centre
        else:
            # centre + R (p - centre), R the rotation of the pixel's offset (row, column) by the angle `turn` (a
            # turn is 2 pi radians)
            turn = 2 * np.pi * params.amount * along
            matrix = np.array([[np.cos(turn), np.sin(turn)], [-np.sin(turn), np.cos(turn)]])
            offset = centre - matrix @ centre
        for total, plane in zip(totals, planes, strict=True):
            # constant: order 1 is bilinear interpolation: the weighted mean of the four pixels around the point
            total += ndimage.affine_transform(plane, matrix, offset=offset, order=1, mode=_BEYOND_THE_EDGE[params.edge])
    return np.clip(np.stack(totals, axis=-1) / params.samples, rgb.min(), rgb.max()).astype(np.float32)
