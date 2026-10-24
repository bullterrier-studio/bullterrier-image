"""The `starfield` effect: a generated night sky of stars at different depths; it replaces the picture."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import screen
from photo_fx.ops.particles import draw_segments


class StarfieldParams(EffectParams):
    """Starfield: stars of different sizes and brightnesses scattered over a dark sky."""

    count: int = param(
        600,
        unit="count",
        min=0,
        max=20000,
        doc="How many stars there are, whatever the size of the picture (a number, not a density). 0 = an empty sky, "
        "600 a clear night.",
    )
    size: float = param(
        3.0,
        unit="px",
        min=1,
        max=50,
        doc="The diameter in px of the nearest, biggest stars (the others are smaller by their depth). Tuned for a "
        "picture about 480 px high. Larger = bigger stars.",
    )
    depth: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How far apart the stars are in depth, which spreads their size and brightness: a star is between 1 - "
        "depth and 1 times the full size and brightness. 0 = every star alike, 1 = from the nearest down to specks "
        "too small to see.",
    )
    color: Color = param(
        "#dfe8ff",
        doc="The colour of the stars' light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a cold white. Stars "
        "are screened over the sky, so they only lighten it.",
    )
    sky: Color = param(
        "#04060f",
        doc="The colour of the sky between the stars: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a "
        "blue-black night.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same stars in the same places, another seed others.",
    )


@effect(category="backgrounds", params=StarfieldParams)
def starfield(rgb: np.ndarray, params: StarfieldParams) -> np.ndarray:
    """A night sky of `count` stars of `color` over `sky`: a generated picture that replaces the input.

    The input is used only for its height and width; its content does not show. Every star has a random depth u from 0
    to 1 and a scale s = 1 - `depth` x u, so near stars (u = 0) are full size and brightness: it is a dot `size` x s
    wide, s bright, with antialiased edges. Where stars overlap the brighter one shows. Stars are screened over the
    sky, so they only lighten it; the same `seed` always gives the same sky.
    """
    height, width = rgb.shape[:2]
    # constant: three random numbers per star, its x, y and depth
    x, y, far = np.random.default_rng(params.seed).random((params.count, 3)).T
    scale = 1 - params.depth * far
    centre = np.stack([x * width, y * height], axis=1)
    # constant: a one-pixel antialiased edge
    light = draw_segments((height, width), centre, centre, params.size / 2 * scale, scale, 1.0)
    sky = np.zeros((height, width, 3), np.float32) + np.asarray(params.sky, np.float32)
    return screen(sky, light[..., None] * np.asarray(params.color, np.float32))
