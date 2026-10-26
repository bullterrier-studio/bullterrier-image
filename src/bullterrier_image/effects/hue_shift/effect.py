"""The `hue_shift` effect: every hue turned around the colour wheel, brightness kept."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import LUMA

# Rec. 709 luma/chroma basis built from the same weights as `luminance`: rows give Y, Cb, Cr from R, G, B.
_KR, _KG, _KB = LUMA
_LUMA_CHROMA = np.array(
    [
        [_KR, _KG, _KB],
        [-_KR / (2 * (1 - _KB)), -_KG / (2 * (1 - _KB)), 0.5],
        [0.5, -_KG / (2 * (1 - _KR)), -_KB / (2 * (1 - _KR))],
    ]
)
_LUMA_CHROMA_INVERSE = np.linalg.inv(_LUMA_CHROMA)


class HueShiftParams(EffectParams):
    """How far every hue turns around the colour wheel."""

    hue: float = param(
        90.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        drivable=True,
        doc="The turn around the colour wheel: 120 sends red to green, 360 is a full turn (unchanged).",
    )


@effect(category="colour", params=HueShiftParams)
def hue_shift(rgb: np.ndarray, params: HueShiftParams) -> np.ndarray:
    """Rotate every hue around the colour wheel by `hue` degrees: the quickest way to try another colour scheme.

    Brightness is preserved (as `luminance` measures it), so the picture keeps its lighting while colours change,
    except for very saturated colours whose rotated version cannot be displayed and gets clipped.
    """
    c, s = np.cos(np.radians(params.hue)), np.sin(np.radians(params.hue))
    rotate_chroma = np.array([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])  # hue = angle in the (Cb, Cr) plane
    matrix = (_LUMA_CHROMA_INVERSE @ rotate_chroma @ _LUMA_CHROMA).astype(np.float32)
    return np.clip(rgb @ matrix.T, 0.0, 1.0).astype(np.float32)
