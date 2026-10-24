"""The `threshold` effect: a two-colour bitmap split at a brightness level."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance


class ThresholdParams(EffectParams):
    """Where the brightness is split, and the two colours the halves are painted in."""

    level: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Brightness (Rec. 709 luminance, 0 = black, 1 = white) at which the picture is split: pixels at or "
        "above it are painted `light`, the rest `dark`. Raising it turns more of the picture dark; 0 makes all of it "
        "light.",
    )
    dark: Color = param(
        "#000000",
        doc="Colour of the pixels darker than `level`: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    light: Color = param(
        "#ffffff",
        doc="Colour of the pixels at or above `level`: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )


@effect(category="colour", params=ThresholdParams)
def threshold(rgb: np.ndarray, params: ThresholdParams) -> np.ndarray:
    """Turn the picture into a two-colour bitmap: `light` where its brightness reaches `level`, `dark` elsewhere.

    Brightness is Rec. 709 luminance, so a pure green is far brighter than a pure blue, as the eye sees them.
    """
    is_light = (luminance(rgb) >= np.float32(params.level))[..., None]
    return np.where(is_light, np.asarray(params.light, np.float32), np.asarray(params.dark, np.float32))
