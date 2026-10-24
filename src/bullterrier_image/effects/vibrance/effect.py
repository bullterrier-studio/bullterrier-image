"""The `vibrance` effect: raise the saturation of muted colours more than that of vivid ones."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class VibranceParams(EffectParams):
    """How much the saturation of muted colours is raised."""

    amount: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How far muted colours are pushed toward full saturation (0..1): 0 changes nothing, 1 raises a colour's "
        "saturation s to s + s * (1 - s), so a colour at 0.2 goes to 0.36 while one at 0.9 only reaches 0.99. Greys "
        "and fully saturated colours stay as they are; colours are never made duller.",
    )


@effect(category="colour", params=VibranceParams)
def vibrance(rgb: np.ndarray, params: VibranceParams) -> np.ndarray:
    """Raise the saturation of muted colours more than that of vivid ones, never lowering anyone's.

    Works on HSV saturation s = (max - min) / max of the gamma-encoded values. Each colour's saturation becomes
    s + amount * s * (1 - s), which never exceeds 1, so the result stays inside 0..1 without clipping. The strongest
    channel (HSV value) is kept and every other channel moves away from it by the same factor, so the hue is exactly
    the input's (up to float rounding) and a grey, whose saturation is 0, is unchanged.
    """
    strongest = rgb.max(axis=-1, keepdims=True)
    weakest = rgb.min(axis=-1, keepdims=True)
    saturation = np.where(strongest > 0, (strongest - weakest) / np.where(strongest > 0, strongest, 1.0), 0.0)
    spread = 1.0 + np.float32(params.amount) * (1.0 - saturation)  # the factor by which saturation grows
    return np.clip(strongest - spread * (strongest - rgb), 0.0, 1.0).astype(np.float32)
