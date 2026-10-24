"""The `film_grain` effect: grain whose strength follows the brightness, independent in each colour channel."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.noise import clumped_noise


class FilmGrainParams(EffectParams):
    """Film grain: noise per channel, strong in the shadows and weaker in the highlights (or the other way round)."""

    shadows: float = param(
        0.1,
        unit="fraction",
        min=0,
        max=1,
        doc="Standard deviation of the grain at black, in 0..1 picture units: 0 = none, 0.1 is visible, 0.25 is heavy. "
        "Real film is grainiest in the shadows.",
    )
    highlights: float = param(
        0.03,
        unit="fraction",
        min=0,
        max=1,
        doc="Standard deviation of the grain at white, in 0..1 picture units. The grain between black and white "
        "runs linearly from `shadows` to this, by the pixel's brightness; set equal to `shadows` for an even grain, "
        "above it for grain that grows toward the highlights.",
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
        doc="Random seed: the same seed always gives the same grain (a different one in each channel), another seed a "
        "different pattern.",
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


@effect(category="texture", params=FilmGrainParams)
def film_grain(rgb: np.ndarray, params: FilmGrainParams) -> np.ndarray:
    """Film grain: noise per channel whose deviation runs from `shadows` at black to `highlights` at white.

    Each channel gets its own noise (clumped to `size` px, deviation 1) multiplied by the deviation at the pixel's
    brightness, its Rec. 709 luminance, `shadows` + (`highlights` - `shadows`) x luminance. The result is clipped to
    0..1; the same `seed` always gives the same grain.
    """
    deviation = params.shadows + (params.highlights - params.shadows) * luminance(rgb)
    grain = np.stack(
        [
            # constant: two low bits of each channel's seed number the channel, so channels never share a pattern
            clumped_noise(
                rgb.shape[:2],
                size=params.size,
                seed=(params.seed << 2) | channel,
                truncate=params.truncate,
                edge=params.edge,
            )
            for channel in range(rgb.shape[2])
        ],
        axis=-1,
    )
    return np.clip(rgb + deviation[..., None] * grain, 0.0, 1.0).astype(np.float32)
