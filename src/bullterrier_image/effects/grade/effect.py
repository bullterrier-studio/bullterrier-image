"""The `grade` effect: basic tone controls (exposure, contrast around a pivot, saturation, vibrance)."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance


class GradeParams(EffectParams):
    """Basic tone controls: exposure, then contrast, then saturation, then vibrance."""

    exposure: float = param(
        0.0,
        unit="stops",
        min=-10,
        max=10,
        drivable=True,
        doc="Brightness in photographic stops: +1 doubles every value, -1 halves it.",
    )
    contrast: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        drivable=True,
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
        drivable=True,
        doc="Colourfulness: 1 = unchanged, 0 = grey (Rec. 709 brightness kept), above 1 more vivid.",
    )
    vibrance: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="How far muted colours are pushed toward full saturation (0..1), after `saturation`: 0 changes nothing, 1 "
        "raises a colour's saturation s to s + s * (1 - s), so a colour at 0.2 goes to 0.36 while one at 0.9 only "
        "reaches 0.99. Greys and fully saturated colours stay as they are; colours are never made duller.",
    )


@effect(category="colour", params=GradeParams)
def grade(rgb: np.ndarray, params: GradeParams) -> np.ndarray:
    """Basic tone controls: `exposure` in stops, then `contrast` around `pivot`, then `saturation`, then `vibrance`.

    The result is clipped to 0..1 after the contrast (before measuring brightness for the saturation), after the
    saturation (before the vibrance) and at the end.

    `vibrance` works on HSV saturation s = (max - min) / max of the gamma-encoded values. Each colour's saturation
    becomes s + vibrance * s * (1 - s), which never exceeds 1. The strongest channel (HSV value) is kept and every
    other channel moves away from it by the same factor, so the hue is the input's (up to float rounding) and a grey,
    whose saturation is 0, is unchanged.
    """
    out = rgb * np.float32(2.0**params.exposure)  # a stop is a factor of 2 (photographic definition)
    out = params.pivot + (out - params.pivot) * np.float32(params.contrast)
    grey = luminance(np.clip(out, 0, 1))[..., None]
    out = np.clip(grey + (out - grey) * np.float32(params.saturation), 0.0, 1.0)
    strongest = out.max(axis=-1, keepdims=True)
    weakest = out.min(axis=-1, keepdims=True)
    chroma = np.where(strongest > 0, (strongest - weakest) / np.where(strongest > 0, strongest, 1.0), 0.0)
    spread = 1.0 + np.float32(params.vibrance) * (1.0 - chroma)  # the factor by which saturation grows
    return np.clip(strongest - spread * (strongest - out), 0.0, 1.0).astype(np.float32)
