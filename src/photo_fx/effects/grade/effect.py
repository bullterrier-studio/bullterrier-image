"""The `grade` effect: basic tone controls (exposure, contrast around a pivot, saturation)."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance


class GradeParams(EffectParams):
    """Basic tone controls: exposure, then contrast, then saturation."""

    exposure: float = param(
        0.0,
        unit="stops",
        min=-10,
        max=10,
        doc="Brightness in photographic stops: +1 doubles every value, -1 halves it.",
    )
    contrast: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How far values are pushed away from the pivot: 1 = unchanged, 2 = twice as far, 0 = flat at the pivot.",
    )
    pivot: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) that contrast leaves where it is; values above it brighten, below it darken.",
    )
    saturation: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="Colourfulness: 1 = unchanged, 0 = grey (Rec. 709 brightness kept), above 1 more vivid.",
    )


@effect(category="colour", params=GradeParams)
def grade(rgb: np.ndarray, params: GradeParams) -> np.ndarray:
    """Basic tone controls: `exposure` in stops, then `contrast` around `pivot`, then `saturation`.

    The result is clipped to 0..1 after the contrast (before measuring brightness for the saturation) and at the end.
    """
    out = rgb * np.float32(2.0**params.exposure)  # a stop is a factor of 2 (photographic definition)
    out = params.pivot + (out - params.pivot) * np.float32(params.contrast)
    grey = luminance(np.clip(out, 0, 1))[..., None]
    out = grey + (out - grey) * np.float32(params.saturation)
    return np.clip(out, 0.0, 1.0).astype(np.float32)
