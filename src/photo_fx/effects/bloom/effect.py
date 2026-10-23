"""The `bloom` effect: light bleeding from the highlights at several scales, a bright core with a wide soft halo."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.light import highlights, screen


class BloomParams(EffectParams):
    """A multi-scale bloom: the highlights are blurred at `octaves` scales, each half the last, and screened back."""

    radius: float = param(
        32.0,
        unit="px",
        min=0,
        max=500,
        doc="Gaussian sigma of the widest halo in px at the recipe's size: larger spreads the light further; 0 keeps "
        "the light on the highlights themselves. Each further octave is half as wide.",
    )
    threshold: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1, per channel) where the bloom starts: only values above it give light, rising to "
        "full at 1. Raising it limits the bloom to the brightest highlights.",
    )
    amount: float = param(
        0.7,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the halo is screened over the picture: 0 = off, 1 = the halo at full brightness; above 1 "
        "it saturates to white sooner.",
    )
    octaves: int = param(
        4,
        unit="count",
        min=1,
        max=8,
        doc="How many scales the halo is built from: octave 1 has the sigma `radius`, each next one half of the "
        "one before (radius / 2, radius / 4...). 1 = a single Gaussian, as in glow; more adds a tighter, brighter "
        "core around the highlights.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weight each octave gets relative to the wider one before it (the weights are persistence^0, "
        "persistence^1... scaled to add up to 1): 0 = the widest octave alone, 1 = every octave equally. Lower "
        "values give a softer, more diffuse bloom; higher values a tighter, more concentrated one.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where each octave's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is more "
        "exact and slower.",
    )
    mode: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the halo's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), "
        "'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
    )


@effect(category="light", params=BloomParams)
def bloom(rgb: np.ndarray, params: BloomParams) -> np.ndarray:
    """Light bleeding from what is brighter than `threshold`, blurred at `octaves` scales and screened over the picture.

    The highlights are blurred with sigma `radius`, `radius / 2`, `radius / 4`... (`octaves` of them), weighted by
    `persistence` per octave and averaged, then screened over the picture by `amount`. It only ever brightens, keeps
    the colour of the light (each channel blooms on its own) and leaves black black.
    """
    bright = highlights(rgb, params.threshold)
    weights = params.persistence ** np.arange(params.octaves)
    weights /= weights.sum()  # the halo is an average, so an even picture stays as bright as its highlights
    halo = np.zeros_like(bright)
    for octave, weight in enumerate(weights):
        sigma = params.radius / 2.0**octave
        halo += float(weight) * gaussian_blur(bright, sigma, truncate=params.truncate, mode=params.mode)
    return screen(rgb, params.amount * halo)
