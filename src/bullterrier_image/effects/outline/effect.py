"""The `outline` effect: a coloured stroke, and optionally a soft halo, around a shape: the subject, or any field."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.distance import signed_distance


class OutlineParams(EffectParams):
    """The settings of `outline`: the shape, the stroke (width, colour, side) and the halo beyond it."""

    field: FieldName = param(
        "subject",
        doc="The shape outlined: `subject` (the subject's silhouette), `background`, `light`, `depth`, or a region of "
        "the recipe (text, a logo, a drawn shape). A pixel is inside it from `field_threshold` up.",
    )
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
        doc="Which side of the shape's edge the stroke lies on: 'outside' (around the shape, whose own pixels "
        "stay untouched), 'inside' (painted over the shape's rim) or 'center' (half of `width` on each side).",
    )
    glow: float = param(
        0.0,
        unit="px",
        min=0,
        max=200,
        doc="How far a soft halo of the same colour reaches beyond the stroke, in px; it fades smoothly from the "
        "stroke to nothing. Raising it spreads the halo (a large value makes an aura). 0 = no halo.",
    )
    field_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The field's value, 0 to 1, from which a pixel belongs to the shape whose edge is outlined: 0.5 = more "
        "than half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.",
        advanced=True,
    )


@effect(category="subject", params=OutlineParams)
def outline(rgb: np.ndarray, params: OutlineParams, *, field: np.ndarray) -> np.ndarray:
    """A coloured stroke of `width` px around a shape (the subject by default), with an optional soft halo of `glow` px.

    The stroke follows the shape's edge on the side `position` says; 'outside' leaves the shape's own pixels exactly
    as they are, like a sticker's border, and a big `glow` with a thin stroke makes an aura. Where the shape is soft
    the stroke lies behind it. The edge is antialiased over one pixel.

    `field` is filled in by the recipe from the selection `params.field` names: float32 (h, w), 0..1.
    """
    p = params
    signed = signed_distance(field >= p.field_threshold)  # > 0 outside the shape, < 0 inside it
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
    # Outside the shape the stroke lies behind it, so a soft edge shows the stroke only where the shape is absent.
    alpha = np.where(signed > 0, covered * (1.0 - field), stroke_in)
    tint = np.asarray(p.color, np.float32)
    return (rgb + (tint - rgb) * alpha[..., None]).astype(np.float32)
