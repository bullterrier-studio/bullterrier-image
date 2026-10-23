"""The `posterize` effect: few tone levels per channel."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class PosterizeParams(EffectParams):
    """How many tone levels each colour channel keeps."""

    levels: int = param(
        256,
        unit="count",
        min=2,
        max=256,
        doc="Number of evenly spaced tone levels kept in each of red, green and blue, black and white included: "
        "2 gives pure black or full intensity per channel (8 colours), 4 gives 0, 1/3, 2/3 and 1. Lowering it "
        "flattens the tones into fewer, coarser bands; 256 only rounds to the 8-bit levels of a JPEG or PNG.",
    )


@effect(category="colour", params=PosterizeParams)
def posterize(rgb: np.ndarray, params: PosterizeParams) -> np.ndarray:
    """Reduce every colour channel to `levels` evenly spaced tones, each value snapping to the nearest one.

    The levels run from 0 to 1 in steps of 1 / (levels - 1), so black and white are always kept. Doing it twice
    changes nothing more.
    """
    steps = np.float32(params.levels - 1)
    return (np.rint(rgb * steps) / steps).astype(np.float32)
