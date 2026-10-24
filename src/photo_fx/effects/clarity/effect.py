"""The `clarity` effect: local contrast against a Gaussian-blurred copy of the picture."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur


class ClarityParams(EffectParams):
    """Local contrast against a Gaussian-blurred copy of the picture (an unsharp mask with a large radius)."""

    radius: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="Size of each area's 'surroundings': the Gaussian sigma of the blur, in px at the recipe's size. Larger "
        "compares bigger regions; 0 turns the effect off.",
    )
    amount: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How strongly each area's difference from its surroundings is exaggerated: 0 = off, 1 = doubled.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact "
        "and slower.",
    )
    edge: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), "
        "'constant' (constant_value).",
    )
    constant_value: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.",
    )


@effect(category="light", params=ClarityParams)
def clarity(rgb: np.ndarray, params: ClarityParams) -> np.ndarray:
    """Local contrast: exaggerate how each area differs from its surroundings, so features stand out even when
    the whole face sits in a narrow band of brightness. `radius` = size of 'surroundings' in px, `amount` 0 = off.
    Put it before effects that turn tone into marks (ascii, halftone), which are blind to small tonal differences.
    """
    blurred = gaussian_blur(
        rgb, params.radius, truncate=params.truncate, edge=params.edge, constant_value=params.constant_value
    )
    return np.clip(rgb + params.amount * (rgb - blurred), 0.0, 1.0).astype(np.float32)
