"""The `melt` effect: bright parts of the picture run downward in drips, each column by its own length."""

from typing import Literal

import numpy as np
from scipy.special import ndtr

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.noise import fractal
from bullterrier_image.ops.warp import remap


class MeltParams(EffectParams):
    """A melt: every pixel shows the picture from higher up in its own column, the more so the brighter it is."""

    amount: float = param(
        40.0,
        unit="px",
        min=0,
        max=500,
        doc="The longest drip, in px: a white pixel in the columns that drip most is pulled down by this much, "
        "less bright pixels by their brightness's share of it, black ones not at all. 0 leaves the picture "
        "unchanged.",
    )
    scale: float = param(
        12.0,
        unit="px",
        min=1,
        max=200,
        doc="How wide a drip is in px: neighbouring columns drip alike over about this distance. Small = a fringe of "
        "thin needles, large = a few broad runs.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same drips, another seed a different pattern of long and "
        "short columns.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the pixels above and below it "
        "(smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbours (sharpest, a little slower, "
        "and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the drips read above the picture's top edge (only the top `amount` rows are pulled from there): "
        "'nearest' (the top row repeated, so the top smears down), 'reflect' (mirrored, edge pixel repeated), "
        "'mirror' (mirrored about the edge pixel), 'wrap' (the bottom of the picture).",
        advanced=True,
    )


@effect(category="distortion", params=MeltParams)
def melt(rgb: np.ndarray, params: MeltParams) -> np.ndarray:
    """Pull the bright parts of each column downward, up to `amount` px, by a drip length that differs per column.

    Each column has a drip length between 0 and 1, smooth noise along the row of `scale` px (made from a Gaussian
    noise through the normal CDF, so lengths spread evenly), and a pixel is pulled down by `amount` x the column's
    length x its own brightness (Rec. 709 luminance): it shows the picture from that many px higher up in the same
    column, read by `interpolation` with `edge` above the top. Nothing moves sideways, black pixels stay in place, and
    the same `seed` gives the same drips.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    # constant: one octave of noise on a strip one pixel high, so the other two layer settings have no effect
    strip = fractal((1, width), scale=params.scale, octaves=1, persistence=0.0, lacunarity=1.0, seed=params.seed)
    pulled = params.amount * ndtr(strip[0]) * luminance(rgb)
    rows = np.arange(height)[:, None] - pulled
    cols = np.broadcast_to(np.arange(width), (height, width))
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
