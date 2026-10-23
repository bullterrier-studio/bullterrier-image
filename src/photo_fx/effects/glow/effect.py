"""The `glow` effect: a soft bloom spreading from the bright parts of the picture."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.light import highlights, screen


class GlowParams(EffectParams):
    """A bloom: the parts brighter than a threshold are blurred into a halo and screened over the picture."""

    radius: float = param(
        24.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the halo in px at the recipe's size: larger spreads the light further and softer; 0 "
        "keeps the light on the bright parts themselves.",
    )
    amount: float = param(
        0.6,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the halo is screened over the picture: 0 = off, 1 = the halo at full brightness; above 1 "
        "it saturates to white sooner.",
    )
    threshold: float = param(
        0.55,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1, per channel) where the glow starts: only values above it light the halo, rising "
        "to full at 1. Raising it limits the glow to the brightest highlights.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the halo's Gaussian is cut off, in sigmas: the halo reaches truncate x radius px. Larger is more "
        "exact and slower.",
    )
    edge: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the halo's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), "
        "'constant' (constant_value, so the glow fades toward the edge when it is 0).",
    )
    constant_value: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The highlight (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.",
    )


@effect(category="light", params=GlowParams)
def glow(rgb: np.ndarray, params: GlowParams) -> np.ndarray:
    """Soft bloom around the bright parts: what is brighter than `threshold` is blurred by `radius` px and screened
    over the picture by `amount`.

    Glow only ever brightens, and keeps the colour of the light: each channel glows on its own.
    """
    bright = highlights(rgb, params.threshold)
    halo = gaussian_blur(
        bright, params.radius, truncate=params.truncate, mode=params.edge, constant_value=params.constant_value
    )
    return screen(rgb, params.amount * halo)
