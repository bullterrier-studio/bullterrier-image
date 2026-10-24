"""The `sharpen` effect: an unsharp mask that adds back the detail a blur takes away."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur


class SharpenParams(EffectParams):
    """An unsharp mask: the difference between the picture and a blurred copy is added back, scaled by `amount`."""

    radius: float = param(
        2.0,
        unit="px",
        min=0,
        max=100,
        doc="Gaussian sigma of the blurred copy in px at the recipe's size: small radii sharpen fine texture, large "
        "ones add local contrast around coarse edges; 0 leaves the picture unchanged.",
    )
    amount: float = param(
        0.8,
        unit="ratio",
        min=0,
        max=5,
        doc="How much of the detail is added back: 0 = off, 1 = the full difference from the blurred copy; above 1 "
        "edges overshoot and halos show.",
    )
    threshold: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The smallest difference (0..1, per channel) from the blurred copy that is sharpened: smaller differences "
        "are left alone, which keeps grain and smooth skin from being sharpened; 0 sharpens everything.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more "
        "exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=SharpenParams)
def sharpen(rgb: np.ndarray, params: SharpenParams) -> np.ndarray:
    """Sharpen with an unsharp mask: add `amount` times the detail that a `radius` px Gaussian blur removes.

    Detail smaller than `threshold` is left as it is; each colour channel is sharpened on its own, and the result is
    kept within 0..1.
    """
    blurred = gaussian_blur(rgb, params.radius, truncate=params.truncate, edge=params.edge)
    detail = rgb - blurred
    detail *= np.abs(detail) >= params.threshold  # smaller differences count for nothing
    return np.clip(rgb + params.amount * detail, 0.0, 1.0).astype(np.float32)
