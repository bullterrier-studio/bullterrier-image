"""The `soft_focus` effect: a blurred copy screened over the sharp picture, the dreamy look of an Orton glow."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.light import screen


class SoftFocusParams(EffectParams):
    """An Orton-style soft focus: a blurred copy of the picture screened over it."""

    radius: float = param(
        12.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the blurred copy in px at the recipe's size: larger is dreamier and softer; 0 blurs "
        "nothing, so the picture is only brightened by screening it with itself.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the blur's Gaussian is cut off, in sigmas: it reaches truncate x radius px. Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=SoftFocusParams, opacity=0.5)
def soft_focus(rgb: np.ndarray, params: SoftFocusParams) -> np.ndarray:
    """A dreamy glow over the whole picture: a copy blurred by `radius` px is screened over the sharp one.

    This is the Orton effect of film photography (a sharp and a blurred exposure of the same scene stacked). The
    picture only ever gets brighter, detail stays under the glow, and black stays black; the step's `opacity` mixes
    in less of it.
    """
    return screen(rgb, gaussian_blur(rgb, params.radius, truncate=params.truncate, edge=params.edge))
