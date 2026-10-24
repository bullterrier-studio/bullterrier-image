"""The `vignette` effect: the picture fades toward a colour at its edges and corners, the centre left alone."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect


class VignetteParams(EffectParams):
    """A vignette: from `radius` out, the picture is mixed toward `color`, reaching `amount` of it `feather` later."""

    amount: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the edges are mixed toward the colour where the vignette is fullest: 0 = off, 0.5 = halfway, "
        "1 = the colour itself at the corners.",
    )
    radius: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the vignette starts, as a fraction of the distance from the centre to a corner: everything "
        "inside is untouched. Raising it clears more of the picture; 1 leaves even the corners alone.",
    )
    feather: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far beyond the radius the vignette takes to reach full strength, as a fraction of the distance from "
        "the centre to a corner (a smooth S-curve). 0 = a hard edge; raising it softens the transition.",
    )
    roundness: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The shape of the vignette's contours: 1 = an ellipse that follows the picture's proportions, 0 = a "
        "rectangle, which darkens the middle of each edge as much as a corner; in between, a rounded rectangle.",
    )
    color: Color = param(
        "#000000",
        doc="The colour the edges fade to: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens, white lightens.",
    )


@effect(category="light", params=VignetteParams)
def vignette(rgb: np.ndarray, params: VignetteParams) -> np.ndarray:
    """Fade the picture toward `color` from `radius` out to its corners, the centre untouched.

    A pixel's distance from the centre is 0 in the middle and 1 at a corner; the contours of equal distance are
    superellipses through the corners (`roundness` 1: an ellipse, 0: a rectangle). From `radius` the pixel is mixed
    toward `color`, rising over `feather` along a smoothstep to `amount`.
    """
    height, width = rgb.shape[:2]
    across = np.abs(2.0 * np.arange(width) + 1.0 - width) / width  # 0 in the middle column, near 1 at the edge
    down = np.abs(2.0 * np.arange(height) + 1.0 - height) / height
    far, near = np.maximum(down[:, None], across[None, :]), np.minimum(down[:, None], across[None, :])
    # The norm with exponent p, scaled so that a corner is at 1 (p = 2 / roundness; p = inf is the largest coordinate).
    # Dividing by the larger coordinate first keeps the power from underflowing when p is large.
    exponent = 2.0 / params.roundness if params.roundness > 0 else np.inf
    ratio = near / np.where(far > 0, far, 1.0)
    distance = far * ((ratio**exponent + 1.0) / 2.0) ** (1.0 / exponent)
    if params.feather > 0:
        t = np.clip((distance - params.radius) / params.feather, 0.0, 1.0)
    else:
        t = (distance > params.radius).astype(np.float64)
    # constant: 3t^2 - 2t^3 is the standard smoothstep Hermite curve
    shade = (params.amount * t * t * (3.0 - 2.0 * t))[..., None]
    tint = np.asarray(params.color, np.float32)
    return (rgb * (1.0 - shade) + tint * shade).astype(np.float32)
