"""The `relight` effect: the picture's depth read as a relief and lit from one side, the lighting multiplying the
picture's own colours."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.relief import shade


class RelightParams(EffectParams):
    """A relight: depth as height, smoothed by `blur`, lit from `angle` at `elevation`, as a grey gain."""

    angle: float = param(
        315.0,
        unit="degrees",
        min=0,
        max=360,
        doc="Where the light comes from, in degrees clockwise from the top of the picture: 0 = above, 90 = the right, "
        "180 = below, 270 = the left, 315 = the upper left. Sides that face it are brightened, those that face "
        "away darkened.",
    )
    elevation: float = param(
        45.0,
        unit="degrees",
        min=1,
        max=90,
        doc="How high the light stands above the picture's plane, in degrees: 90 = straight above (every slope only "
        "dims a little, whatever `angle`), 45 = the usual raking light, 1 = grazing, so the shapes are dramatic and "
        "the lit side is clipped to white.",
    )
    height: float = param(
        200.0,
        unit="px",
        min=0,
        max=1000,
        doc="How tall the whole depth range stands as a relief, in px at the recipe's size: the nearest depth (1) "
        "stands this many px above the farthest (0), so the slope between neighbours is their depth difference "
        "times this. 0 = flat (the picture is unchanged), larger = steeper shapes that catch more of the light.",
    )
    blur: float = param(
        2.0,
        unit="px",
        min=0,
        max=50,
        doc="Gaussian sigma in px with which the depth is smoothed before its slopes are taken: 0 = every ripple of "
        "the depth map is lit (its noise shows), larger = softer lighting that follows only the broad shapes.",
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


@effect(category="light", params=RelightParams)
def relight(rgb: np.ndarray, params: RelightParams, *, depth: np.ndarray | None = None) -> np.ndarray:
    """Light the picture's depth, read as a relief `height` px tall, from `angle` at `elevation`.

    The lighting comes from the shape of the depth, not from the picture's brightness: the height is the depth
    smoothed by `blur`; the surface normal is (-slope_x, -slope_y, 1) / its length with the slopes taken per pixel by
    central differences. The light's strength is the normal's dot product with the light's direction in proportion to
    what a flat surface gets, so flat areas are unchanged and a slope is brightened or darkened. All channels are
    multiplied by that one grey gain, so the picture's colour ratios are kept, and the result is clipped to 0..1.
    A sharp step in depth (a subject's outline) lights as a steep cliff and can leave a dark or bright fringe on the
    background beside it; raise `blur` or lower `height` to soften it.
    `depth` is filled in by the recipe from the depth model (never set it in a recipe file): float32 (h, w), 0..1,
    1 = nearest. Without a depth the picture is unchanged.
    """
    if depth is None:
        return rgb.copy()
    gain = shade(
        depth,
        height=params.height,
        angle=params.angle,
        elevation=params.elevation,
        blur=params.blur,
        truncate=params.truncate,
        edge=params.edge,
    )
    return np.clip(rgb * gain[..., None], 0, 1).astype(np.float32)
