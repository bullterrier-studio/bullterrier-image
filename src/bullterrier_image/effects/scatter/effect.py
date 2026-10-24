"""The `scatter` effect: every pixel swapped for one picked at random from its neighbourhood."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import remap


class ScatterParams(EffectParams):
    """A scatter: each pixel shows the picture at a random whole-pixel move from its own place."""

    radius: int = param(
        4,
        unit="px",
        min=0,
        max=500,
        doc="How far a pixel may be moved, in whole pixels, up or down and left or right: each pixel shows the one up "
        "to this many pixels away along each direction, picked at random. 0 leaves the picture unchanged; a few pixels "
        "give frosted glass, tens of pixels dissolve the picture into confetti. Tuned for a picture about 480 px high.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same scatter, another seed a different one.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a pixel near the border sees when its move reads from beyond the picture: 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=ScatterParams)
def scatter(rgb: np.ndarray, params: ScatterParams) -> np.ndarray:
    """Replace every pixel by one picked at random from within `radius` px of it.

    Each pixel reads the picture at its own place moved by a random whole number of pixels, between -`radius` and
    `radius`, down and right, drawn independently for every pixel from `seed`. No colours are blended, so every output
    pixel is exactly an input pixel; `edge` says what a move that leaves the picture reads.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    down = rng.integers(-params.radius, params.radius + 1, size=(height, width))
    right = rng.integers(-params.radius, params.radius + 1, size=(height, width))
    rows = np.arange(height)[:, None] + down
    cols = np.arange(width)[None, :] + right
    return remap(rgb, rows, cols, interpolation="nearest", edge=params.edge)
