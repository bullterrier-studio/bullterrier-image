"""The `floating_dust` effect: faint out-of-focus specks of light drifting in front of the picture, like bokeh."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import screen
from photo_fx.ops.particles import draw_segments


class FloatingDustParams(EffectParams):
    """Floating dust: soft round specks of light of different sizes, as dust in a sunbeam looks out of focus."""

    count: int = param(
        80,
        unit="count",
        min=0,
        max=1000,
        doc="How many specks there are, whatever the size of the picture (a number, not a density). 0 = none (the "
        "picture unchanged), 80 is a scattering, 400 a dusty air.",
    )
    size: float = param(
        24.0,
        unit="px",
        min=1,
        max=200,
        doc="The diameter in px of the nearest, largest specks; the farthest are half as wide. Tuned for a picture "
        "about 480 px high. Larger = bigger bokeh discs, as if the dust were closer to the lens.",
    )
    softness: float = param(
        0.8,
        unit="fraction",
        min=0,
        max=1,
        doc="How soft a speck's edge is: the edge falls from full brightness to none across this fraction of the "
        "speck's radius (at least one pixel). 0 = crisp discs, 1 = specks fading out from their middle.",
    )
    opacity: float = param(
        0.55,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright the nearest specks are: the share of `color` they screen over the picture at their middle; "
        "the farthest are fainter. 0 = invisible, 1 = solid colour.",
    )
    color: Color = param(
        "#fff1d6",
        doc="The colour of the light the specks catch: '#rrggbb', '#rgb' or [r, g, b] 0..1. Specks are screened over "
        "the picture, so they only lighten it.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same specks in the same places, another seed others.",
    )


@effect(category="atmosphere", params=FloatingDustParams)
def floating_dust(rgb: np.ndarray, params: FloatingDustParams) -> np.ndarray:
    """Screen `count` soft round specks of `color` at random places over the picture, as out-of-focus dust.

    Every speck has a random depth from 0 (far) to 1 (near): its width and brightness are `size` and `opacity` times
    0.5 + 0.5 x depth. A speck is full brightness inside, half of it at its radius, with an edge `softness` of its
    radius soft. Where specks overlap the brighter one shows. Specks are screened over the picture, so they only
    lighten it; the same `seed` always gives the same dust.
    """
    height, width = rgb.shape[:2]
    # constant: three random numbers per particle, its x, y and depth
    x, y, depth = np.random.default_rng(params.seed).random((params.count, 3)).T
    near = 0.5 + 0.5 * depth
    centre = np.stack([x * width, y * height], axis=1)
    radius = params.size / 2 * near
    light = draw_segments((height, width), centre, centre, radius, params.opacity * near, params.softness * radius)
    return screen(rgb, light[..., None] * np.asarray(params.color, np.float32))
