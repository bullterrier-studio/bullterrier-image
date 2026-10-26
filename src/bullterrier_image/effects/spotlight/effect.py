"""The `spotlight` effect: the subject stays lit and the rest of the picture darkens with distance from it."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.distance import signed_distance


class SpotlightParams(EffectParams):
    """The settings of `spotlight`: how dark the surroundings get, how fast, how far, and how softly."""

    amount: float = param(
        0.8,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the picture is darkened where it is darkest, far from the subject: 0 = unchanged, 1 = fully "
        "`color`.",
    )
    falloff: float = param(
        0.8,
        unit="ratio",
        min=0.1,
        max=10,
        doc="How fast the darkening grows with distance from the subject: it is (distance / radius)^falloff of its "
        "full strength. 1 = evenly, below 1 darkens quickly near the subject, above 1 stays light near it and "
        "darkens late.",
    )
    radius: float = param(
        200.0,
        unit="px",
        min=1,
        max=5000,
        doc="The distance from the subject's silhouette, in px at the recipe's size, at which the darkening reaches "
        "full strength; beyond it the picture stays that dark. Raising it spreads the light further.",
    )
    feather: float = param(
        10.0,
        unit="px",
        min=0,
        max=200,
        doc="How softly the light spills from the subject, as a Gaussian sigma in px: the area around the silhouette "
        "stays lit through this much of its surroundings. Raising it makes a softer pool of light round the "
        "subject; 0 = the darkening starts right at the silhouette.",
    )
    color: Color = param(
        "#000000",
        doc="The colour the surroundings darken toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens; a "
        "dark blue gives a night look.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the `feather` blur is cut off, in sigmas: it reaches truncate x feather px. Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the `feather` blur sees beyond the picture's border: 'reflect' (mirrored, border pixel repeated), "
        "'mirror' (mirrored about the border pixel), 'nearest' (the border pixel repeated) or 'wrap' (the opposite "
        "border).",
        advanced=True,
    )
    subject_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The subject's value, 0 to 1, from which a pixel belongs to the subject that stays lit: 0.5 = more than "
        "half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.",
        advanced=True,
    )


@effect(category="subject", params=SpotlightParams)
def spotlight(rgb: np.ndarray, params: SpotlightParams, *, subject: np.ndarray | None = None) -> np.ndarray:
    """Light the subject and darken the rest: the further from the silhouette, the more the picture goes to `color`.

    Darkness is `amount` x (distance / `radius`)^`falloff` (full from `radius` px out), times the share of the
    surroundings that `feather` leaves unlit, so a feathered spotlight bleeds light softly past the silhouette. The
    subject itself is never touched. With a subject that covers nothing, everything is the rest and is darkened fully.

    `subject` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject to light and the picture is returned unchanged.
    """
    if subject is None:
        return rgb.copy()
    p = params
    outside = np.maximum(signed_distance(subject >= p.subject_threshold), 0.0)  # px from the subject; 0 on and in it
    spill = gaussian_blur(subject, p.feather, truncate=p.truncate, edge=p.edge)  # the light that reaches past the edge
    darkness = p.amount * np.clip(outside / p.radius, 0.0, 1.0) ** p.falloff * (1.0 - spill)
    tint = np.asarray(p.color, np.float32)
    return (rgb + (tint - rgb) * darkness[..., None]).astype(np.float32)
