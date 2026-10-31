"""The `grade` effect: colour balance, exposure, contrast around a pivot, saturation and vibrance.

Its parameters are declared by the Rust struct `Params` in `crates/core/src/effects/grade.rs`.
"""

import numpy as np

from bullterrier_image._native import _core
from bullterrier_image.core.params import EffectParams
from bullterrier_image.core.registry import effect


class GradeParams(EffectParams, core="grade"):  # noqa: D101  (documented by its Rust declaration)
    pass


@effect(params=GradeParams)
def grade(rgb: np.ndarray, params: GradeParams) -> np.ndarray:
    """White balance, exposure, contrast, saturation and vibrance: `temperature`/`tint` fix a warm or cool cast first.

    Order: colour balance, `exposure`, `contrast` around `pivot`, `saturation`, `vibrance`.

    The colour balance is von Kries scaling: the picture is decoded to linear light (sRGB, IEC 61966-2-1), each channel
    is multiplied by a gain, and it is encoded again. The gains are red 2**(temperature/2), green 2**-tint and blue
    2**(-temperature/2), divided by the largest of the three, so the balance never brightens or clips and white keeps
    one full channel instead of turning into a cast. Temperature and tint at 0 leave the picture as it was.

    The result is clipped to 0..1 after the contrast (before measuring brightness for the saturation), after the
    saturation (before the vibrance) and at the end.

    `vibrance` works on HSV saturation s = (max - min) / max of the gamma-encoded values. Each colour's saturation
    becomes s + vibrance * s * (1 - s), which never exceeds 1. The strongest channel (HSV value) is kept and every
    other channel moves away from it by the same factor, so the hue is the input's (up to float rounding) and a grey,
    whose saturation is 0, is unchanged.
    """
    return _core.apply("grade", rgb, params.model_dump())
