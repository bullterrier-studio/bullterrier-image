"""The `grain` effect: film grain, seeded brightness noise that can clump into larger specks."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur


class GrainParams(EffectParams):
    """Film grain: monochrome brightness noise of a set standard deviation, optionally clumped by a Gaussian blur."""

    amount: float = param(
        0.08,
        unit="fraction",
        min=0,
        max=1,
        doc="Standard deviation of the added brightness, in 0..1 picture units: 0 = none, 0.08 is visible, 0.2 is "
        "heavy.",
    )
    size: float = param(
        1.0,
        unit="px",
        min=0,
        max=50,
        doc="Grain size in px: up to 1 every pixel varies on its own; above 1 the noise is blurred (Gaussian sigma = "
        "size / 2) into specks about this wide, keeping its standard deviation. Larger = coarser, softer grain.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same grain, another seed a different pattern.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the clumping blur's Gaussian is cut off, in sigmas (only when size > 1). Larger is more exact and "
        "slower.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the clumping blur sees beyond the picture's edge (only when size > 1): 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge, so the grain tiles seamlessly).",
    )


@effect(category="texture", params=GrainParams)
def grain(rgb: np.ndarray, params: GrainParams) -> np.ndarray:
    """Film grain: brightness noise of standard deviation `amount` (0.08 visible, 0.2 heavy), clumped to `size` px.

    The same noise is added to every channel, so the grain is monochrome; the result is clipped to 0..1. The same
    `seed` always gives the same grain, so renders are repeatable.
    """
    noise = np.random.default_rng(params.seed).standard_normal(rgb.shape[:2]).astype(np.float32)
    if params.size > 1.0:  # a speck of 1 px or less is a single pixel: nothing to clump
        noise = gaussian_blur(noise, params.size / 2.0, truncate=params.truncate, edge=params.edge)
        noise /= max(float(noise.std()), 1e-6)  # constant: guard against dividing by zero on a 1-pixel picture
    return np.clip(rgb + params.amount * noise[..., None], 0.0, 1.0).astype(np.float32)
