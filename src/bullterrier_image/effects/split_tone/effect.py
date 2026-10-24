"""The `split_tone` effect: one tint for the shadows, another for the highlights, brightness left alone."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance


class SplitToneParams(EffectParams):
    """Two tints, one for the shadows and one for the highlights, that leave brightness alone."""

    shadow: Color = param("#3a1bff", doc="The tint of the dark parts: '#rrggbb', '#rgb' or [r, g, b] 0..1.")
    highlight: Color = param("#ff2bd6", doc="The tint of the bright parts: '#rrggbb', '#rgb' or [r, g, b] 0..1.")
    amount: float = param(
        0.3, unit="fraction", min=0, max=1, doc="How strongly the tints are added: 0 = off, 0.2 a hint, 0.6 strong."
    )
    balance: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) where the shadow tint hands over to the highlight tint. Raising it tints more of "
        "the picture with the shadow colour.",
    )
    softness: float = param(
        0.5,
        unit="fraction",
        min=0.01,
        max=1,
        doc="Width of the brightness band (0.01..1), centred on balance, over which one tint blends into the other. "
        "Smaller makes a sharper hand-over.",
    )
    black_fade: float = param(
        0.1,
        unit="fraction",
        min=0.001,
        max=1,
        doc="The brightness (0.001..1) up to which the tints fade in from nothing, so pure black stays black. "
        "Larger keeps more of the shadows untinted.",
    )


@effect(category="colour", params=SplitToneParams)
def split_tone(rgb: np.ndarray, params: SplitToneParams) -> np.ndarray:
    """Tint the dark parts with `shadow` and the bright parts with `highlight`, without changing brightness.

    The subtle way to get a neon or cinematic cast: skin tones and detail stay, the colour shifts toward the tints.
    `amount` 0..1 (0.2 is a hint, 0.6 is strong), `balance` = the brightness where one tint hands over to the other.
    Pure black stays black (a tint fades in up to `black_fade` brightness), so a black background is not coloured.
    Near pure white a tint would clip, so those few pixels shift slightly in brightness.
    """
    lum = luminance(rgb)
    t = np.clip(0.5 + (lum - params.balance) / params.softness, 0.0, 1.0)[..., None]  # 0.5: halfway, at balance
    tint = np.asarray(params.shadow, np.float32) * (1.0 - t) + np.asarray(params.highlight, np.float32) * t
    offset = tint - luminance(tint)[..., None]  # the tint's colour with its brightness taken out
    fade_in = np.clip(lum / params.black_fade, 0.0, 1.0)[..., None]  # nothing is added to pure black
    return np.clip(rgb + params.amount * offset * fade_in, 0.0, 1.0).astype(np.float32)
