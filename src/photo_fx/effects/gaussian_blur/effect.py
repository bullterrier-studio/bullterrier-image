"""The `gaussian_blur` effect: the picture softened by a Gaussian of a chosen radius."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur as blur


class GaussianBlurParams(EffectParams):
    """A Gaussian blur: every pixel becomes a weighted average of its neighbours, the weights a bell curve."""

    radius: float = param(
        4.0,
        unit="px",
        min=0,
        max=500,
        doc="The Gaussian's standard deviation (sigma) in px at the recipe's size: larger is softer and spreads detail "
        "further; 0 leaves the picture unchanged.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact "
        "and slower; below 3 the blur turns visibly boxy.",
    )
    edge: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), "
        "'constant' (constant_value, so the edge fades toward it).",
    )
    constant_value: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1, every channel) assumed beyond the edge when edge is 'constant': 0 darkens the "
        "edge, 1 lightens it; ignored otherwise.",
    )


@effect(category="light", params=GaussianBlurParams)
def gaussian_blur(rgb: np.ndarray, params: GaussianBlurParams) -> np.ndarray:
    """Soften the picture with a Gaussian blur of standard deviation `radius` px.

    Each colour channel is blurred on its own. The blur keeps flat areas flat and never leaves the picture's range.
    """
    return blur(rgb, params.radius, truncate=params.truncate, edge=params.edge, constant_value=params.constant_value)
