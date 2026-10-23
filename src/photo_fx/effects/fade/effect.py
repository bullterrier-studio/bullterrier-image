"""The `fade` effect: lifted blacks, the matte look of faded film."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class FadeParams(EffectParams):
    """How far the blacks are lifted."""

    amount: float = param(
        0.12,
        unit="fraction",
        min=0,
        max=1,
        doc="How far black is lifted toward white (0..1): the darkest value the picture can reach. 0 = unchanged, "
        "0.1 = a gentle matte look, 1 = everything white. White stays white; raising it flattens the shadows.",
    )


@effect(category="colour", params=FadeParams)
def fade(rgb: np.ndarray, params: FadeParams) -> np.ndarray:
    """Lift the blacks by `amount` for a matte, faded-film look; whites stay where they are.

    Every channel is rescaled from 0..1 to `amount`..1, so black becomes a flat grey of `amount`, white is untouched
    and the tones between keep their order. With `amount` 0 the picture is unchanged.
    """
    lift = np.float32(params.amount)
    return (lift + rgb * (1.0 - lift)).astype(np.float32)
