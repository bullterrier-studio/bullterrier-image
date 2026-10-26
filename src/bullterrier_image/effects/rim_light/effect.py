"""The `rim_light` effect: a thin band of coloured light along the subject's edge, on the side facing the light."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.distance import signed_distance
from bullterrier_image.ops.light import screen


class RimLightParams(EffectParams):
    """The settings of `rim_light`: where the light comes from, its colour, and how deep and strong the rim is."""

    angle: float = param(
        40.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="The direction the light comes from, counter-clockwise from the right: 0 = from the right, 90 = from "
        "above, 180 or -180 = from the left, -90 = from below. Only edges whose outward direction faces it are lit.",
    )
    color: Color = param(
        "#ff9a4d",
        doc="The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm orange.",
    )
    width: float = param(
        12.0,
        unit="px",
        min=1,
        max=200,
        doc="Depth of the lit band inside the subject's edge, in px at the recipe's size: raising it lets the light "
        "reach further in.",
    )
    amount: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How strong the light is where it is brightest (an edge that faces the light head-on): 0 = off, 1 = the "
        "colour at full strength, above 1 saturates sooner.",
    )
    falloff: float = param(
        1.5,
        unit="ratio",
        min=0.1,
        max=10,
        doc="How the light fades from the edge into the band: it is (1 - depth / width)^falloff of its strength. 1 = "
        "evenly, above 1 stays close to the edge, below 1 carries further in.",
    )
    smoothing: float = param(
        2.0,
        unit="px",
        min=0,
        max=50,
        doc="Gaussian sigma, in px, with which the silhouette is smoothed before the edge's direction is read: "
        "raising it evens out the jaggedness of a pixel edge (fur), at the cost of detail in small features. 0 uses "
        "the raw edge.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the `smoothing` blur is cut off, in sigmas: it reaches truncate x smoothing px. Larger is more "
        "exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the `smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border pixel repeated), "
        "'mirror' (mirrored about the border pixel), 'nearest' (the border pixel repeated) or 'wrap' (the opposite "
        "border).",
        advanced=True,
    )
    subject_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The subject's value, 0 to 1, from which a pixel belongs to the subject whose edge is lit: 0.5 = more than "
        "half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.",
        advanced=True,
    )


@effect(category="subject", params=RimLightParams)
def rim_light(rgb: np.ndarray, params: RimLightParams, *, subject: np.ndarray | None = None) -> np.ndarray:
    """Light along the subject's edge from one side: a band of `width` px inside the edge, tinted `color`.

    Only the edge that faces the light is lit, by the cosine of the angle between its outward direction and the
    direction of `angle` (head-on = full, side-on = none, away = none), and the light fades with depth by `falloff`.
    It is screened over the picture, so it only ever brightens, and never reaches the background or the subject's
    interior beyond `width`. The picture's own border is no edge.

    `subject` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one, or
    with a subject that has no edge (empty or full), there is nothing to light and the picture is returned unchanged.
    """
    if subject is None:
        return rgb.copy()
    p = params
    inside = subject >= p.subject_threshold
    signed = signed_distance(inside)  # > 0 outside the subject, < 0 inside it, infinite without an edge
    if not np.isfinite(signed).all():
        return rgb.copy()
    # The outward direction of the edge is the way the smoothed distance grows.
    smooth = gaussian_blur(signed, p.smoothing, truncate=p.truncate, edge=p.edge)
    grad_y, grad_x = np.gradient(smooth)
    length = np.maximum(np.hypot(grad_x, grad_y), 1e-6)  # constant: guards a flat field (no direction, no light)
    toward_light = np.radians(p.angle)
    # x runs right and y down, so a light above the picture has a negative y.
    facing = np.maximum((grad_x * np.cos(toward_light) - grad_y * np.sin(toward_light)) / length, 0.0)
    depth = -signed  # px from the edge into the subject; 1 on its outermost pixels
    band = np.clip(1.0 - (depth - 1.0) / p.width, 0.0, 1.0) ** p.falloff
    light = p.amount * facing * np.where(inside, band * subject, 0.0)
    tint = np.asarray(p.color, np.float32)
    return screen(rgb, light[..., None] * tint)
