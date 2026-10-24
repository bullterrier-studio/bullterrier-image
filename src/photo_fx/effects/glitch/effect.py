"""The `glitch` effect: colour channels pulled apart and horizontal bands knocked sideways."""

from typing import Self

import numpy as np
from pydantic import model_validator

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class GlitchParams(EffectParams):
    """The settings of `glitch`: the sliding bands and the red/blue split."""

    split: int = param(
        10,
        unit="px",
        min=-500,
        max=500,
        doc="How far red is pushed right and blue left, -500 to 500 px (negative swaps the directions); 0 = no "
        "colour split. The picture wraps around at the edges.",
    )
    bands: int = param(
        7,
        unit="count",
        min=0,
        max=500,
        doc="How many random horizontal strips slide sideways, 0 to 500; 0 = none.",
    )
    shift: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        doc="The furthest a strip slides, as a fraction 0 to 1 of the picture's width; each strip slides a random "
        "distance up to this, left or right, wrapping around.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same damage; any other gives different strips.",
    )
    band_min_height: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=1,
        doc="The lowest a strip can be, as a fraction 0 to 1 of the picture's height (never below 1 px). Each strip's "
        "height is drawn at random between this and `band_max_height`, which it may not exceed.",
    )
    band_max_height: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        doc="The tallest a strip can be, as a fraction 0 to 1 of the picture's height. Raising it gives taller strips.",
    )

    @model_validator(mode="after")
    def _band_heights_in_order(self) -> Self:
        """The band height limits are a range: refused, naming both, when the lowest is above the tallest."""
        if self.band_min_height > self.band_max_height:
            raise ValueError(
                f"`band_min_height` ({self.band_min_height}) must not exceed `band_max_height` "
                f"({self.band_max_height}): they are the lowest and the tallest a strip can be"
            )
        return self


@effect(category="glitch", params=GlitchParams)
def glitch(rgb: np.ndarray, params: GlitchParams) -> np.ndarray:
    """`bands` random horizontal strips slide sideways by up to `shift` of the width, then red and blue are pushed
    `split` px apart.

    The same `seed` always gives the same damage. The colour split runs last, so the bands get the same red and blue
    fringes as the rest of the picture.
    """
    rng = np.random.default_rng(params.seed)
    height, width = rgb.shape[:2]
    out = rgb.copy()
    for _ in range(params.bands):
        band_height = max(1, int(rng.uniform(params.band_min_height, params.band_max_height) * height))
        top = int(rng.uniform(0, height - band_height))
        dx = int(rng.uniform(-params.shift, params.shift) * width)
        out[top : top + band_height] = np.roll(out[top : top + band_height], dx, axis=1)

    out[..., 0] = np.roll(out[..., 0], params.split, axis=1)
    out[..., 2] = np.roll(out[..., 2], -params.split, axis=1)
    return out
