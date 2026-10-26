"""The `grain` effect: film grain, seeded brightness noise that can clump into larger specks."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.noise import clumped_noise


class GrainParams(EffectParams):
    """Film grain: monochrome brightness noise of a set standard deviation, optionally clumped by a Gaussian blur."""

    amount: float = param(
        0.08,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
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
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the clumping blur sees beyond the picture's edge (only when size > 1): 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge, so the grain tiles seamlessly).",
        advanced=True,
    )


@effect(category="texture", params=GrainParams)
def grain(rgb: np.ndarray, params: GrainParams) -> np.ndarray:
    """Film grain: brightness noise of standard deviation `amount` (0.08 visible, 0.2 heavy), clumped to `size` px.

    The same noise is added to every channel, so the grain is monochrome; the result is clipped to 0..1. The same
    `seed` always gives the same grain, so renders are repeatable.
    """
    noise = clumped_noise(rgb.shape[:2], size=params.size, seed=params.seed, truncate=params.truncate, edge=params.edge)
    return np.clip(rgb + params.amount * noise[..., None], 0.0, 1.0).astype(np.float32)
