"""The `noise` effect: random values of a set standard deviation added to every pixel, in grey or in colour."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class NoiseParams(EffectParams):
    """Additive noise: independent random values added to each pixel, mono or per channel."""

    amount: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="Standard deviation of the added noise, in 0..1 picture units: 0 = none, 0.06 is visible, 0.2 is heavy.",
    )
    distribution: Literal["gaussian", "uniform"] = param(
        "gaussian",
        doc="Where the noise values come from: 'gaussian' (bell curve: most values small, a few large) or 'uniform' "
        "(every value up to a limit of about 1.7 x `amount` equally likely: an even, bounded speckle). Both have the "
        "standard deviation `amount`.",
    )
    color: bool = param(
        True,
        doc="True gives every channel its own noise (coloured speckle); False adds the same noise to all three "
        "(brightness-only, grey speckle).",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same noise, another seed a different pattern.",
    )


@effect(category="texture", params=NoiseParams)
def noise(rgb: np.ndarray, params: NoiseParams) -> np.ndarray:
    """Add noise of standard deviation `amount` (0.06 visible, 0.2 heavy) to every pixel, grey or per channel.

    Each pixel gets an independent value, Gaussian or uniform by `distribution` (zero mean, standard deviation
    `amount` either way), the same in all channels unless `color`. The result is clipped to 0..1; the same `seed`
    always gives the same noise.
    """
    rng = np.random.default_rng(params.seed)
    shape = rgb.shape if params.color else rgb[..., :1].shape
    if params.distribution == "gaussian":
        values = rng.standard_normal(shape)
    else:  # uniform on -b..b has standard deviation b / sqrt(3), so b = sqrt(3) gives 1
        values = (rng.random(shape) * 2 - 1) * np.sqrt(3)  # constant: the uniform distribution's standard deviation
    return np.clip(rgb + params.amount * values, 0.0, 1.0).astype(np.float32)
