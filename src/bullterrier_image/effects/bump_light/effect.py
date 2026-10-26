"""The `bump_light` effect: the picture's brightness read as a relief and lit from one side, the lighting multiplying
the picture's own colours."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.relief import shade


class BumpLightParams(EffectParams):
    """A bump light: brightness as height, smoothed by `blur`, lit from `angle` at `elevation`, as a grey gain."""

    angle: float = param(
        315.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="Where the light comes from, in degrees clockwise from the top of the picture: 0 = above, 90 = the right, "
        "180 = below, 270 = the left, 315 = the upper left. Slopes that face it are brightened, those that face "
        "away darkened.",
    )
    elevation: float = param(
        45.0,
        unit="degrees",
        min=1,
        max=90,
        doc="How high the light stands above the picture's plane, in degrees: 90 = straight above (every slope only "
        "dims a little, whatever `angle`), 45 = the usual raking light, 1 = grazing, so the relief is dramatic and the "
        "lit side is clipped to white.",
    )
    height: float = param(
        8.0,
        unit="px",
        min=0,
        max=100,
        doc="How tall the relief is: a white pixel stands this many px above a black one, so the slope between "
        "neighbours is their brightness difference times this. 0 = flat (the picture is unchanged), larger = steeper "
        "relief and stronger lighting.",
    )
    blur: float = param(
        1.0,
        unit="px",
        min=0,
        max=20,
        doc="Gaussian sigma in px with which the brightness is smoothed before it is read as height: 0 = every pixel "
        "is a bump (noise and grain shows), larger = broader, softer swells.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the smoothing Gaussian is cut off, in sigmas (only matters when `blur` > 0). Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel "
        "repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge, so a tileable picture lights without a seam).",
        advanced=True,
    )


@effect(category="texture", params=BumpLightParams)
def bump_light(rgb: np.ndarray, params: BumpLightParams) -> np.ndarray:
    """Light the picture's brightness, read as a relief `height` px tall, from `angle` at `elevation`.

    The height is the Rec. 709 luminance smoothed by `blur`; the surface normal is (-slope_x, -slope_y, 1) / its
    length with the slopes taken per pixel by central differences. The light's strength is the normal's dot product
    with the light's direction in proportion to what a flat surface gets, so flat areas are unchanged and a slope is
    brightened or darkened. All channels are multiplied by that one grey gain, so the picture's colour ratios are
    kept, and the result is clipped to 0..1.
    """
    gain = shade(
        luminance(rgb),
        height=params.height,
        angle=params.angle,
        elevation=params.elevation,
        blur=params.blur,
        truncate=params.truncate,
        edge=params.edge,
    )
    return np.clip(rgb * gain[..., None], 0, 1).astype(np.float32)
