"""The `droste` effect: the picture shown again, smaller and turned, in its own middle, over and over."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class DrosteParams(EffectParams):
    """A Droste effect: a picture that contains a smaller copy of itself, which contains a smaller copy..."""

    scale: float = param(
        0.5,
        unit="ratio",
        min=0.1,
        max=0.9,
        doc="How much smaller each copy is than the one around it, strictly between 0 and 1: 0.5 = each copy is half "
        "the size of the last, 0.9 = a slow shrink with many copies close together, 0.1 = one small copy. The copy "
        "has the picture's own shape and sits in its middle.",
    )
    turn: float = param(
        20.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        doc="How far each copy is turned from the one around it, in degrees: positive turns the copies clockwise as "
        "the picture is seen, negative anticlockwise, 0 leaves them upright (nested frames); other values make the "
        "copies spiral into the middle.",
    )
    repeats: int = param(
        4,
        unit="count",
        min=1,
        max=12,
        doc="How many copies are drawn inside one another at most, 1 to 12: the copy this deep shows the picture's "
        "middle instead of a further copy. Raising it adds ever smaller copies towards the middle.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but the small copies turn blocky), 'bilinear' blends the four pixels around it "
        "(smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little "
        "slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a turned copy shows where it reads from beyond the picture (a turned copy's corners reach past the "
        "edge): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the "
        "edge pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=DrosteParams)
def droste(rgb: np.ndarray, params: DrosteParams) -> np.ndarray:
    """Show the picture again, `scale` times as large and turned by `turn`, in its own middle, up to `repeats` times.

    Measure each pixel's distance from the middle as a share of the picture's half-width or half-height, whichever
    is larger (1 at the edge, so the picture and its copies are rectangles of the picture's shape). A pixel nearer
    than `scale` is inside a copy: it shows the picture at `1 / scale` times its own distance, turned by `turn`, and
    if that is nearer than `scale` again, it is inside a copy of the copy, and so on, `repeats` times at most. The
    outside of the picture is the picture as it is.
    """
    height, width = rgb.shape[:2]
    centre = Centre.middle(height, width)
    down, across = centre.offsets()
    size = np.maximum(np.abs(down) / (max(height - 1, 1) / 2), np.abs(across) / (max(width - 1, 1) / 2))
    # the copy a pixel is in: how many steps of 1 / scale carry its size out to `scale` or more (a pixel in the very
    # middle would need endlessly many, so the count stops at `repeats`)
    nearest = params.scale ** (params.repeats + 1)  # the smallest size that is told apart from 0, as `repeats` allows
    depth = np.ceil(np.log(params.scale / np.maximum(size, nearest)) / -np.log(params.scale))
    depth = np.clip(depth, 0, params.repeats)
    rows, cols = centre.sources(scale=params.scale**-depth, turn=np.radians(params.turn) * depth)
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
