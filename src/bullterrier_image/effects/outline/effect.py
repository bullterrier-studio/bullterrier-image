"""The `outline` effect: a coloured stroke, and optionally a soft halo, around the subject's silhouette."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.distance import signed_distance


class OutlineParams(EffectParams):
    """The settings of `outline`: the stroke (width, colour, side, opacity) and the halo beyond it."""

    width: float = param(
        6.0,
        unit="px",
        min=0,
        max=200,
        doc="Thickness of the stroke in px at the recipe's size; raising it thickens the line. 0 = no stroke.",
    )
    color: Color = param(
        "#ffffff",
        doc="The colour of the stroke and of its halo: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    position: Literal["outside", "inside", "center"] = param(
        "outside",
        doc="Which side of the silhouette's edge the stroke lies on: 'outside' (around the subject, whose own pixels "
        "stay untouched), 'inside' (painted over the subject's rim) or 'center' (half of `width` on each side).",
    )
    glow: float = param(
        0.0,
        unit="px",
        min=0,
        max=200,
        doc="How far a soft halo of the same colour reaches beyond the stroke, in px; it fades smoothly from the "
        "stroke to nothing. Raising it spreads the halo (a large value makes an aura). 0 = no halo.",
    )
    opacity: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How solid the stroke and halo are: 0 = invisible, 1 = fully covering the picture.",
    )
    matte_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The matte value, 0 to 1, from which a pixel belongs to the subject whose edge is outlined: 0.5 = more "
        "than half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.",
        advanced=True,
    )


@effect(category="subject", params=OutlineParams)
def outline(rgb: np.ndarray, params: OutlineParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """A coloured stroke of `width` px around the subject's silhouette, with an optional soft halo of `glow` px.

    The stroke follows the edge of the subject on the side `position` says; 'outside' leaves the subject's own pixels
    exactly as they are, like a sticker's border, and a big `glow` with a thin stroke makes an aura. Where the matte
    is soft the stroke lies behind the subject. The edge is antialiased over one pixel.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject to outline and the picture is returned unchanged.
    """
    if matte is None:
        return rgb.copy()
    p = params
    signed = signed_distance(matte >= p.matte_threshold)  # > 0 outside the subject, < 0 inside it
    outer_share = (p.position == "outside") + (p.position == "center") / 2  # the share of the stroke beyond the edge
    reach_out = p.width * outer_share  # px of stroke beyond the edge
    reach_in = p.width - reach_out  # and the rest of it inside
    # Full coverage up to the reach, then a one-pixel ramp down to nothing (constant: antialiasing over one pixel).
    stroke_out = np.clip(reach_out - signed + 1, 0.0, 1.0)
    stroke_in = np.clip(reach_in + signed + 1, 0.0, 1.0)
    covered = stroke_out
    if p.glow > 0:
        beyond = np.maximum(signed - reach_out, 0.0)  # px past the outer edge of the stroke
        halo = np.clip(1.0 - beyond / p.glow, 0.0, 1.0) ** 2  # constant: quadratic fade, no visible edge at `glow`
        covered = np.maximum(stroke_out, halo)
    # Outside the subject the stroke lies behind it, so a soft edge shows the stroke only where the subject is absent.
    alpha = p.opacity * np.where(signed > 0, covered * (1.0 - matte), stroke_in)
    tint = np.asarray(p.color, np.float32)
    return (rgb + (tint - rgb) * alpha[..., None]).astype(np.float32)
