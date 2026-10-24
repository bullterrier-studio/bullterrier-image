"""The `long_shadow` effect: the flat-design shadow that streams away from the subject in one direction."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.cutout import put_behind
from photo_fx.ops.shift import shift


class LongShadowParams(EffectParams):
    """The settings of `long_shadow`: the direction and length of the shadow, and its colour, opacity and fade."""

    angle: float = param(
        45.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="The direction the shadow is cast, clockwise as seen in the picture: 0 = to the right, 45 = to the lower "
        "right, 90 = down, 180 = to the left, -90 = up. Angles that are not a multiple of 45 leave the shadow's "
        "edges about a pixel soft.",
    )
    length: float = param(
        300.0,
        unit="px",
        min=0,
        max=5000,
        doc="How far the shadow reaches from the subject's outline, in px at the recipe's size; 0 = no shadow (the "
        "silhouette behind itself, which shows only as a faint fringe where the cut-out is soft). Raising it makes "
        "the shadow longer; the effect's time grows only with the logarithm of it.",
    )
    color: Color = param(
        "#000000",
        doc="The colour of the shadow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the "
        "subject; another colour tints it.",
    )
    opacity: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the picture behind the subject the shadow replaces with its colour where it is fullest, at "
        "the subject: 0 = no shadow, 1 = the picture is fully covered there.",
    )
    fade: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of its opacity the shadow loses along its length, in a straight line: 0 = solid to the end, "
        "0.5 = half as opaque at the end, 1 = fades out completely at `length`.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the shadow sees beyond the picture's edge when the silhouette is moved: 'nearest' (the outermost "
        "row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, "
        "edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="subject", params=LongShadowParams)
def long_shadow(rgb: np.ndarray, params: LongShadowParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """A flat-design long shadow: the subject's silhouette repeated along `angle` for `length` px, behind it.

    The shadow is every copy of the silhouette moved along the ray, one per pixel of travel, the farther ones fainter
    by `fade`; it is laid behind the subject in `color` at `opacity`, so the subject itself is never changed.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject and so no shadow: the picture comes back unchanged.
    """
    if matte is None:
        return rgb.copy()
    heading = np.radians(params.angle)
    direction = np.array([np.sin(heading), np.cos(heading)])  # (down, right) as seen in the picture
    per_pixel = direction / np.abs(direction).max()  # one copy per pixel along the axis the ray runs along most
    reach = np.hypot(*per_pixel)  # px between two copies
    copies = int(params.length / reach) + 1  # copy 0 is the silhouette itself
    # The union of the copies is built by doubling: after each round `cast` holds the union of `held` copies, so a
    # round that moves it by `held` copies and keeps the larger value doubles `held`; the fade is a straight ramp
    # (`fade` over `length` px), so the loss over `held` copies is the same wherever those copies start.
    cast, held = matte, 1  # constant: copy 0 is the silhouette itself, so one copy is held to begin with
    while held < copies:
        moved = min(held, copies - held)  # the last round moves by what is still missing
        fainter = shift(cast, *(per_pixel * moved), edge=params.edge) - float(
            params.fade * reach * moved / params.length
        )
        cast = np.maximum(cast, fainter)
        held += moved
    return put_behind(rgb, matte, params.opacity * cast, np.asarray(params.color, np.float32))
