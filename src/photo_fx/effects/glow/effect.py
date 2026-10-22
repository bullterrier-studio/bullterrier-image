"""The `glow` effect: a soft bloom spreading from the bright parts of the picture."""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class GlowParams(EffectParams):
    """A bloom: the parts brighter than a threshold are blurred into a halo and screened over the picture."""

    radius: float = param(
        24.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the halo in px at the recipe's size: larger spreads the light further and softer (and "
        "takes longer); 0 keeps the light on the bright parts themselves.",
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
    mode: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
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
        doc="The highlight (0..1) assumed beyond the edge when mode is 'constant'; ignored otherwise.",
    )


@effect(category="light", params=GlowParams)
def glow(rgb: np.ndarray, params: GlowParams) -> np.ndarray:
    """Soft bloom around the bright parts: what is brighter than `threshold` is blurred by `radius` px and screened
    over the picture by `amount`.

    Glow only ever brightens, and keeps the colour of the light: each channel glows on its own.
    """
    bright = np.clip((rgb - params.threshold) / (1.0 - params.threshold), 0.0, 1.0)
    halo = ndimage.gaussian_filter(
        bright,
        sigma=(params.radius, params.radius, 0),  # no blur across the colour axis
        mode=params.mode,
        cval=params.constant_value,
        truncate=params.truncate,
    )
    return (1.0 - (1.0 - rgb) * (1.0 - np.clip(params.amount * halo, 0, 1))).astype(np.float32)  # screen blend
