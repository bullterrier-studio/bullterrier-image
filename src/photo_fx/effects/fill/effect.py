"""The `fill` effect: one colour over the whole picture (masked to the background, it removes the background)."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect


class FillParams(EffectParams):
    """The colour that replaces the picture."""

    color: Color = param("#000000", doc="The colour that fills the whole picture: '#rrggbb', '#rgb' or [r, g, b] 0..1.")


@effect(category="subject", params=FillParams)
def fill(rgb: np.ndarray, params: FillParams) -> np.ndarray:
    """Replace the whole picture with one colour. Masked to the background, this is 'remove the background'."""
    return np.broadcast_to(np.asarray(params.color, np.float32), rgb.shape).copy()
