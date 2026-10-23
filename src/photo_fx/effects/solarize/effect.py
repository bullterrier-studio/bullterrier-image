"""The `solarize` effect: invert the values above a threshold."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class SolarizeParams(EffectParams):
    """Where the tones start to reverse."""

    threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Channel value (0..1) above which red, green and blue are inverted; values at or below it stay. "
        "Lowering it reverses more of the tones (0 inverts everything but pure black); 1 changes nothing.",
    )


@effect(category="colour", params=SolarizeParams)
def solarize(rgb: np.ndarray, params: SolarizeParams) -> np.ndarray:
    """Invert every red, green and blue value that is above `threshold` and leave the others as they are.

    The tones fold back on themselves at the threshold, the look of an over-exposed darkroom print (the Sabattier
    effect): shadows stay, highlights turn dark, and a bright edge appears where the two meet.
    """
    return np.where(rgb > np.float32(params.threshold), 1.0 - rgb, rgb).astype(np.float32)
