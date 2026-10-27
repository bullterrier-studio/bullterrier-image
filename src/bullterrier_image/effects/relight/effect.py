"""The `relight` effect: a field (the picture's depth, its brightness, any region) read as a relief and lit from one
side, the lighting multiplying the picture's own colours or a mid grey."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.relief import shade


class RelightParams(EffectParams):
    """A relight: a field as height, smoothed by `blur`, lit from `angle` at `elevation`, as a grey gain on a mix of
    the picture and mid grey."""

    field: FieldName = param(
        "depth",
        doc="The field read as the relief, the higher the value the higher the ground: `depth` (the nearest stands "
        "highest; the subject rises from its background), `light` (the picture's own brightness: bright parts "
        "stand proud), `subject`, or a region the recipe declares.",
    )

    angle: float = param(
        225.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="Where the light comes from, in degrees clockwise from pointing right, on screen: 0 = the right, 90 = "
        "below, 180 = the left, 225 = the upper left, 270 = above. Sides that face it are brightened, those that "
        "face away darkened.",
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
        doc="How tall the whole range of the field stands as a relief, in px at the recipe's size: a field value of 1 "
        "stands this many px above 0, so the slope between neighbours is their difference times this. 0 = flat (the "
        "relief is nothing: the picture at `saturation` 1, mid grey at 0), larger = steeper shapes that catch more "
        "of the light. A field of the picture's own brightness wants far less (about 8) than a depth (about 200).",
    )
    blur: float = param(
        2.0,
        unit="px",
        min=0,
        max=50,
        doc="Gaussian sigma in px with which the field is smoothed before its slopes are taken: 0 = every ripple of "
        "the field is lit (its noise shows), larger = softer lighting that follows only the broad shapes.",
    )
    saturation: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the picture's colour is lit: 1 = the picture itself, lit; 0 = the relief alone, a grey "
        "surface lit (a flat field is mid grey); between, the picture mixed into mid grey.",
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
def relight(rgb: np.ndarray, params: RelightParams, *, field: np.ndarray) -> np.ndarray:
    """Light the picture from the shape of a field, read as a relief `height` px tall, from `angle` at `elevation`: bump
    lighting, emboss, relighting by depth.

    The lighting comes from the shape of `field`, not from the picture's brightness unless that is the field: the
    height is the field smoothed by `blur`; the surface normal is (-slope_x, -slope_y, 1) / its length with the slopes
    taken per pixel by central differences. The light's strength is the normal's dot product with the light's direction
    in proportion to what a flat surface gets, so flat areas are unchanged and a slope is brightened or darkened. The
    lit surface is the picture mixed into mid grey by `saturation`; all channels are multiplied by the one grey gain,
    so the colour ratios are kept, and the result is clipped to 0..1. A low `elevation` with `saturation` 0 is an
    emboss: the relief in grey. A sharp step in the field (a subject's outline in depth) lights as a steep cliff and
    can leave a dark or bright fringe beside it; raise `blur` or lower `height` to soften it.
    `field` is filled in by the recipe from the selection `params.field` names: float32 (h, w), 0..1.
    """
    gain = shade(
        field,
        height=params.height,
        angle=params.angle,
        elevation=params.elevation,
        blur=params.blur,
        truncate=params.truncate,
        edge=params.edge,
    )
    mid_grey = 0.5  # constant: the neutral surface the relief is lit on when the picture's colours are left out
    surface = params.saturation * rgb + (1 - params.saturation) * mid_grey
    return np.clip(surface * gain[..., None], 0, 1).astype(np.float32)
