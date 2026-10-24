"""The `soft_focus` effect: a blurred copy screened over the sharp picture, the dreamy look of an Orton glow."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.light import screen


class SoftFocusParams(EffectParams):
    """An Orton-style soft focus: a blurred copy of the picture screened over it, at an opacity."""

    radius: float = param(
        12.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the blurred copy in px at the recipe's size: larger is dreamier and softer; 0 blurs "
        "nothing, so the picture is only brightened by screening it with itself.",
    )
    opacity: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the screened result replaces the sharp picture: 0 = off (the picture unchanged), 1 = the "
        "full soft focus. Raising it brightens and softens more.",
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


@effect(category="light", params=SoftFocusParams)
def soft_focus(rgb: np.ndarray, params: SoftFocusParams) -> np.ndarray:
    """A dreamy glow over the whole picture: a copy blurred by `radius` px is screened over the sharp one.

    `opacity` sets how much of that replaces the sharp picture. This is the Orton effect of film photography (a sharp
    and a blurred exposure of the same scene stacked). The picture only ever gets
    brighter, detail stays under the glow, and black stays black.
    """
    blurred = gaussian_blur(rgb, params.radius, truncate=params.truncate, edge=params.edge)
    return (rgb * (1.0 - params.opacity) + screen(rgb, blurred) * params.opacity).astype(np.float32)
