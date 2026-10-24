"""The `aurora` effect: soft vertical light curtains, brightest at a wavy base, over a dark sky."""

from functools import partial

import numpy as np
from scipy.special import ndtr

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal
from photo_fx.ops.ramp import ramp


class AuroraParams(EffectParams):
    """An aurora: one curtain of light with a wavy base, streaked by vertical rays, fading upward into the sky."""

    colors: tuple[Color, ...] = param(
        ("#2bff9a", "#7a2cff"),
        doc="The curtain's colours from its base to its top, evenly spaced along the way up; at least two. The classic "
        "aurora is green at the base turning violet or red higher up.",
        min_length=2,
    )
    sky: Color = param(
        "#02050f",
        doc="The colour of the night sky, seen below the curtain's base, above its top and between its rays: "
        "'#rrggbb', '#rgb' or [r, g, b] 0..1. A very dark blue looks better than pure black.",
    )
    base: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the curtain's bright lower edge lies, as a fraction of the picture's height from the top (0 = the "
        "top edge, 1 = the bottom edge). Below it the sky is dark. Waviness moves it up and down along the picture.",
    )
    height: float = param(
        0.7,
        unit="fraction",
        min=0.05,
        max=1,
        doc="How far up from its base the curtain reaches before it has faded out completely, as a fraction of the "
        "picture's height. Larger = a taller curtain.",
    )
    falloff: float = param(
        0.8,
        unit="ratio",
        min=0.2,
        max=8,
        doc="How fast the light fades going up: the brightness at a height h above the base, as a fraction of the "
        "curtain's height, is (1 - h) to this power. 1 = a straight fade, above 1 = a bright edge with the glow "
        "quickly falling away, below 1 = light that reaches high before dimming.",
        advanced=True,
    )
    waviness: float = param(
        40.0,
        unit="px",
        min=0,
        max=500,
        doc="How far the curtain's base wanders up and down across the picture, in pixels: the typical distance from "
        "the `base` line (the standard deviation of the ripple); tuned for a picture about 480 px high. 0 = a "
        "straight, level base.",
    )
    scale: float = param(
        200.0,
        unit="px",
        min=2,
        max=2048,
        doc="Size in pixels, along the picture, of the curtain's coarsest ripples: the spacing of the first noise "
        "lattice of the base's wobble. Larger = long, slow folds; small = a quickly rippling hem.",
        advanced=True,
    )
    rays: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How strongly the curtain is broken into vertical rays: each ray has a brightness of its own and this is "
        "how far the dimmest drop below the brightest. 0 = a smooth, even curtain, 1 = rays separated by dark sky.",
    )
    ray_width: float = param(
        10.0,
        unit="px",
        min=1,
        max=256,
        doc="Width in pixels of the broadest rays, the spacing of the first noise lattice across the picture; tuned "
        "for a picture about 480 px high. Larger = a few wide shafts; small = a fine comb of streaks.",
        advanced=True,
    )
    octaves: int = param(
        3,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed for the base's wobble and for the rays, each finer than the last (by "
        "`lacunarity`) and weaker (by `persistence`). 1 = smooth, more = more ragged detail. Layers finer than one "
        "pixel are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer of the wobble and the rays is than the one before: each layer's weight "
        "is this to the power of its number. 0 = only the coarsest layer, 1 = every layer equally strong.",
        advanced=True,
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = "
        "every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
        advanced=True,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same curtain (its folds and its rays), another seed a "
        "different one.",
    )


@effect(category="backgrounds", params=AuroraParams)
def aurora(rgb: np.ndarray, params: AuroraParams) -> np.ndarray:
    """Replace the picture with an aurora: a curtain of light with a wavy base and vertical rays over a dark sky.

    A generated background: the input is used only for its height and width, its content is ignored. In column x the
    curtain's base is the row `base` x height + `waviness` x ripple(x), the ripple being fractal noise of unit
    deviation along the picture (`scale`). A pixel d = (rows above the base) / (`height` x picture height) up the
    curtain has brightness (1 - d)**`falloff`, 0 below the base (a 1 px anti-aliased edge) and above d = 1, times the
    column's ray, 1 - `rays` x (1 - r) with r in 0..1 the rank of a second fractal noise along the picture
    (`ray_width`). Its colour is `colors` at d, mixed into `sky` by that brightness.
    """
    rows, width = rgb.shape[:2]
    # constant: a one-row field, so the noise runs along the picture only
    noise = partial(
        fractal, (1, width), octaves=params.octaves, persistence=params.persistence, lacunarity=params.lacunarity
    )
    ripple = noise(scale=params.scale, seed=params.seed)[0]
    # constant: stream 1 of the seed, independent of the ripple's stream 0 (the seed itself)
    rank = ndtr(noise(scale=params.ray_width, seed=[params.seed, 1]))[0]
    above = (params.base * rows + params.waviness * ripple)[None, :] - np.arange(rows)[:, None]  # px above the base
    up = above / (params.height * rows)  # 0 at the base, 1 at the curtain's top
    brightness = np.clip(above + 0.5, 0.0, 1.0) * np.clip(1 - up, 0.0, 1.0) ** params.falloff
    brightness = brightness * (1 - params.rays * (1 - rank))[None, :]
    colour = ramp(np.clip(up, 0.0, 1.0).astype(np.float32), params.colors)
    sky = np.asarray(params.sky, np.float32)
    return (sky + brightness[..., None] * (colour - sky)).astype(np.float32)
