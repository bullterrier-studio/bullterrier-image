"""The `emboss` effect: the picture's brightness read as a relief and lit from one side, on mid grey."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.relief import lambert, slopes


class EmbossParams(EffectParams):
    """An emboss: brightness as height, lit from `angle`, shaded around mid grey; `saturation` returns the colours."""

    angle: float = param(
        315.0,
        unit="degrees",
        min=0,
        max=360,
        doc="Where the light comes from, in degrees clockwise from the top of the picture: 0 = above, 90 = the right, "
        "180 = below, 270 = the left, 315 = the upper left (the usual choice: it reads as raised). Surfaces that rise "
        "away from the light are brighter than mid grey and those that rise towards it darker; turning the light "
        "by 180 inverts the picture around mid grey.",
    )
    depth: float = param(
        6.0,
        unit="ratio",
        min=0,
        max=50,
        doc="How tall the relief is: brightness change per pixel (0..1) is multiplied by this to give the slope. 0 = "
        "a flat surface (mid grey), larger = steeper relief and stronger shading, up to the full range for sharp "
        "edges. The slope is per pixel, so the same value looks stronger on a smaller picture.",
    )
    saturation: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the picture's colour comes back: 0 = a grey emboss on mid grey, 1 = the picture itself "
        "shaded by the relief (flat areas keep their colour), in between a mix of the two.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the slope sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so a "
        "tileable picture embosses without a seam).",
    )


@effect(category="texture", params=EmbossParams)
def emboss(rgb: np.ndarray, params: EmbossParams) -> np.ndarray:
    """Read the brightness as a relief of slope `depth` x brightness change per px and light it from `angle`.

    The shade is 0.5 + 0.5 x the cosine between the surface normal and a horizontal light, so a flat surface is mid
    grey, one facing the light edge-on 1 and one facing away 0, and the shade never leaves 0..1. `saturation` mixes in
    the picture multiplied by twice the shade (flat areas keep their colour), clipped to 0..1.
    """
    rise_y, rise_x = slopes(luminance(rgb), scale=params.depth, edge=params.edge)
    horizon = 0  # constant: a light level with the picture's plane, so a flat surface catches none and is mid grey
    # The shade is 1 where the surface leans edge-on to the light, 0 where it leans away, 0.5 where it is flat.
    shade = (0.5 + 0.5 * lambert(rise_y, rise_x, angle=params.angle, elevation=horizon)).astype(np.float32)[..., None]
    coloured = np.clip(2 * shade * rgb, 0, 1)
    return ((1 - params.saturation) * shade + params.saturation * coloured).astype(np.float32)
