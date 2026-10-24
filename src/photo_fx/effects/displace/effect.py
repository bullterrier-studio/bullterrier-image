"""The `displace` effect: every pixel moved sideways and up or down by two independent fields of fractal noise."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal
from photo_fx.ops.warp import remap


class DisplaceParams(EffectParams):
    """A displacement: each pixel shows the picture at a point moved by noise, the farthest by `amount`."""

    amount: float = param(
        12.0,
        unit="px",
        min=0,
        max=500,
        doc="The farthest any pixel's move is, in pixels: the moves are scaled so that the largest one, wherever the "
        "noise is strongest, is exactly this far. Raising it tears the picture up more; 0 leaves it unchanged.",
    )
    scale: float = param(
        80.0,
        unit="px",
        min=1,
        max=1024,
        doc="Size of the coarsest waves of the displacement in pixels, the spacing of the first noise lattice. Larger "
        "= broad, slow wobbles that bend the picture; small = fine, jittery tearing.",
    )
    octaves: int = param(
        3,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed, each finer than the last (by `lacunarity`) and weaker (by "
        "`persistence`). 1 = smooth waves, more = more fine, ragged detail. Layers finer than one pixel are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before: each layer's weight is this to the power of its "
        "number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = "
        "every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same displacement, another seed a different one.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a pixel near the border sees when its move reads from beyond the picture: 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge).",
    )


@effect(category="distortion", params=DisplaceParams)
def displace(rgb: np.ndarray, params: DisplaceParams) -> np.ndarray:
    """Move every pixel by two independent noise fields (down and right), the longest move being `amount` px.

    Each pixel shows the picture at its own place plus a move (rows, columns) from two fractal noise fields of the
    same `scale`, `octaves`, `persistence` and `lacunarity`, drawn from `seed` independently of each other. The moves
    are scaled together, by one number, so that the longest of them is `amount` px: no pixel reads from farther than
    that, so the `edge` rule only matters within `amount` px of the border.
    """
    shape = rgb.shape[:2]
    moves = [
        fractal(
            shape,
            scale=params.scale,
            octaves=params.octaves,
            persistence=params.persistence,
            lacunarity=params.lacunarity,
            seed=2 * params.seed + axis,  # two seeds per `seed`, so the fields are independent
        )
        for axis in range(2)
    ]
    # Each field has unit spread, so (unless the picture is a single pixel, where it is all 0) a move of at least 1
    # exists somewhere: dividing by the longer of that and 1 makes the longest move exactly `amount`.
    longest = max(np.hypot(*moves).max(), 1)
    rows = np.arange(shape[0])[:, None] + params.amount * moves[0] / longest
    cols = np.arange(shape[1])[None, :] + params.amount * moves[1] / longest
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
