"""The `drop_shadow` effect: a soft shadow of the subject on the picture behind it."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.cutout import put_behind
from bullterrier_image.ops.shift import shift


class DropShadowParams(EffectParams):
    """The settings of `drop_shadow`: where the shadow falls, how soft it is, and its colour."""

    dx: float = param(
        12.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far the shadow is moved to the right, in px at the recipe's size; negative moves it left. 0 with "
        "`dy` 0 hides the shadow behind the subject, leaving only what the blur spreads past its edge.",
    )
    dy: float = param(
        18.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far the shadow is moved down, in px at the recipe's size; negative moves it up.",
    )
    blur: float = param(
        24.0,
        unit="px",
        min=0,
        max=200,
        doc="Gaussian sigma of the shadow's soft edge in px: raising it makes the shadow larger and fainter at its "
        "edge; 0 = a hard-edged copy of the subject's silhouette.",
    )
    color: Color = param(
        "#000000",
        doc="The colour of the shadow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the "
        "subject; another colour tints it.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the shadow's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the shadow sees beyond the picture's edge, when it is moved and softened: 'nearest' (the outermost "
        "row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, "
        "edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="subject", params=DropShadowParams, opacity=0.6)
def drop_shadow(rgb: np.ndarray, params: DropShadowParams, *, subject: np.ndarray | None = None) -> np.ndarray:
    """A soft shadow of the subject cast on the picture behind it, moved by `dx`, `dy` and softened by `blur`.

    The shadow is the subject's silhouette moved and blurred, laid over the background in `color`: where it is full
    it replaces the picture, where its blurred edge thins out it shows less. The subject itself is never changed, and
    the shadow only ever shows where the subject is not.

    `subject` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject and so no shadow: the picture comes back unchanged.
    """
    if subject is None:
        return rgb.copy()
    cast = shift(subject, params.dy, params.dx, edge=params.edge)
    cast = gaussian_blur(cast, params.blur, truncate=params.truncate, edge=params.edge)
    return put_behind(rgb, subject, cast, np.asarray(params.color, np.float32))
