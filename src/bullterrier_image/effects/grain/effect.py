"""The `grain` effect: seeded noise whose strength follows the brightness, grey or per colour channel, clumpable."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.noise import clumped_noise


class GrainParams(EffectParams):
    """Grain: noise of a set standard deviation, heavier in the shadows or the highlights by `balance`, optionally
    clumped into specks and optionally different in each colour channel."""

    amount: float = param(
        0.08,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="Standard deviation of the grain at mid-grey, in 0..1 picture units: 0 = none, 0.08 is visible, 0.2 is "
        "heavy. With `balance` the grain at black and at white is up to 2 x this.",
    )
    balance: float = param(
        0.0,
        unit="ratio",
        min=-1,
        max=1,
        drivable=True,
        doc="Where the grain is heavier, from -1 to 1: -1 = only in the shadows, 0 = even, 1 = only in the "
        "highlights. The grain at brightness L (0 black, 1 white) is `amount` x (1 + `balance` x (2 L - 1)), so the "
        "shadows get `amount` x (1 - `balance`) and the highlights `amount` x (1 + `balance`). Real film is grainiest "
        "in the shadows (a negative balance).",
    )
    size: float = param(
        1.0,
        unit="px",
        min=0,
        max=50,
        doc="Grain size in px: up to 1 every pixel varies on its own; above 1 the noise is blurred (Gaussian sigma = "
        "size / 2) into specks about this wide, keeping its standard deviation. Larger = coarser, softer grain.",
    )
    color: bool = param(
        False,
        doc="True gives every channel its own grain (coloured speckle, like a high-ISO sensor); False adds the same "
        "grain to all three (brightness only, grey speckle).",
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
    """Grain: noise of standard deviation `amount` at mid-grey, clumped to `size` px, heavier where `balance` says.

    The noise is multiplied by the deviation at the pixel's brightness, its Rec. 709 luminance L: `amount` x
    (1 + `balance` x (2 L - 1)). It is the same in all channels (grey grain) unless `color`, which gives
    each channel its own. The result is clipped to 0..1; the same `seed` always gives the same grain.
    """
    deviation = params.amount * (1 + params.balance * (2 * luminance(rgb) - 1))
    # constant: two low bits of each channel's seed number the channel, so channels never share a pattern
    seeds = [(params.seed << 2) | channel for channel in range(rgb.shape[2])] if params.color else [params.seed]
    noise = np.stack(
        [
            clumped_noise(rgb.shape[:2], size=params.size, seed=seed, truncate=params.truncate, edge=params.edge)
            for seed in seeds
        ],
        axis=-1,
    )
    return np.clip(rgb + deviation[..., None] * noise, 0.0, 1.0).astype(np.float32)
