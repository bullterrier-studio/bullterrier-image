"""The `black_and_white` effect: a monochrome conversion with red, green and blue filter weights."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import LUMA

# constant: only keeps an all-zero filter (nothing passes) from dividing by zero; it gives black.
_NO_LIGHT = 1e-12


class BlackAndWhiteParams(EffectParams):
    """How much of each colour channel goes into the grey, as the colour filter of black and white film does."""

    red: float = param(
        LUMA[0],
        unit="ratio",
        min=0,
        max=1,
        doc="Weight of the red channel in the grey, 0..1. Raising it lightens reds and oranges (skin, red roses) and "
        "darkens blues. The default is the Rec. 709 luma weight of red.",
    )
    green: float = param(
        LUMA[1],
        unit="ratio",
        min=0,
        max=1,
        doc="Weight of the green channel in the grey, 0..1. Raising it lightens greens (foliage). The default is the "
        "Rec. 709 luma weight of green.",
    )
    blue: float = param(
        LUMA[2],
        unit="ratio",
        min=0,
        max=1,
        doc="Weight of the blue channel in the grey, 0..1. Raising it lightens blues (sky, water) and darkens skin. "
        "The default is the Rec. 709 luma weight of blue.",
    )


@effect(category="colour", params=BlackAndWhiteParams)
def black_and_white(rgb: np.ndarray, params: BlackAndWhiteParams) -> np.ndarray:
    """Convert to black and white with `red`, `green` and `blue` filter weights, like a colour filter on B&W film.

    The weights are shares: they are divided by their sum, so only their ratio matters and a grey picture comes back
    unchanged (the result is clipped only to absorb float rounding). The default weights are the Rec. 709 luma
    weights, applied to the gamma-encoded values as the rest of bullterrier-image measures brightness. If all three
    weights are 0 no light passes the filter and the result is black.
    """
    weights = np.array([params.red, params.green, params.blue], dtype=np.float32)
    weights = weights / max(float(weights.sum()), _NO_LIGHT)
    grey = rgb @ weights
    return np.repeat(np.clip(grey, 0.0, 1.0)[..., None], 3, axis=-1).astype(np.float32)
