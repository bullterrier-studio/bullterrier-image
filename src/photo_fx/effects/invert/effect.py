"""The `invert` effect: the photographic negative."""

import numpy as np

from photo_fx.core.params import EffectParams
from photo_fx.core.registry import effect


class InvertParams(EffectParams):
    """The negative has no settings: every channel value v becomes 1 - v."""


@effect(category="colour", params=InvertParams)
def invert(rgb: np.ndarray, params: InvertParams) -> np.ndarray:
    """Turn the picture into its negative: every red, green and blue value v becomes 1 - v.

    Black becomes white, white black, mid grey stays mid grey and each colour becomes its complement, so a warm
    picture turns cool. Doing it twice gives back the original picture.
    """
    return (1.0 - rgb).astype(np.float32)
