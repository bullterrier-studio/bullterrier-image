"""The `halation` effect: the red-orange glow around bright highlights that film adds when light bounces back."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.light import highlights, screen


class HalationParams(EffectParams):
    """Film halation: the brightest parts of the picture are blurred, tinted and screened back over it."""

    radius: float = param(
        20.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the glow in px at the recipe's size: larger spreads the glow further from the "
        "highlights; 0 keeps it on the highlights themselves.",
    )
    threshold: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1) where the glow starts, judged on the brightest of a pixel's three channels: only "
        "values above it glow, rising to full at 1. Raising it limits the halation to the brightest highlights.",
    )
    amount: float = param(
        0.7,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the tinted glow is screened over the picture: 0 = off, 1 = the glow at full brightness; "
        "above 1 it saturates sooner.",
    )
    color: Color = param(
        "#ff4a14",
        doc="The colour of the glow: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the red-orange of film's "
        "red-sensitive layer; white makes a neutral glow.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the glow's Gaussian is cut off, in sigmas: it reaches truncate x radius px. Larger is more exact "
        "and slower.",
    )
    mode: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the glow's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), "
        "'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
    )


@effect(category="light", params=HalationParams)
def halation(rgb: np.ndarray, params: HalationParams) -> np.ndarray:
    """A coloured glow around the highlights: where the brightest channel is above `threshold` it is blurred by
    `radius` px, tinted `color` and screened over the picture by `amount`.

    Real halation comes from light that passes through the film base and bounces back, exposing the red-sensitive
    layer around bright spots, so the glow takes the tint whatever the colour of the highlight itself. It only ever
    brightens and leaves black black.
    """
    brightest = highlights(rgb.max(axis=-1), params.threshold)
    halo = gaussian_blur(brightest, params.radius, truncate=params.truncate, mode=params.mode)
    tint = np.asarray(params.color, np.float32)
    return screen(rgb, (params.amount * halo)[..., None] * tint)
