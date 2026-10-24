"""The `highpass` effect: only the fine detail of the picture, centred on mid-grey."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur

MID_GREY = 0.5  # constant: the level the detail is centred on: halfway between black and white


class HighpassParams(EffectParams):
    """A high-pass filter: the picture minus a blurred copy of it, shown around mid-grey."""

    radius: float = param(
        4.0,
        unit="px",
        min=0,
        max=100,
        doc="Gaussian sigma of the blurred copy in px at the recipe's size: detail finer than this is kept, coarser "
        "tones are removed. Small radii keep only the finest texture, large ones keep edges and local contrast; 0 "
        "keeps nothing but mid-grey.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more "
        "exact and slower.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="light", params=HighpassParams)
def highpass(rgb: np.ndarray, params: HighpassParams) -> np.ndarray:
    """Keep only the detail finer than `radius` px: the picture minus its Gaussian blur, centred on mid-grey.

    A flat area becomes mid-grey, an edge a dark line on its dark side and a bright one on its bright side. Each
    colour channel is filtered on its own and the result is kept within 0..1.
    """
    blurred = gaussian_blur(rgb, params.radius, truncate=params.truncate, edge=params.edge)
    return np.clip(rgb - blurred + MID_GREY, 0.0, 1.0).astype(np.float32)
