"""The `lens_blur` effect: the picture as an out-of-focus lens draws it, every point spread into a disc."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.convolve import convolve


class LensBlurParams(EffectParams):
    """A lens blur: every pixel becomes the average of the picture over a disc around it."""

    radius: float = param(
        8.0,
        unit="px",
        min=0,
        max=100,
        doc="The radius of the disc in px at the recipe's size: larger is further out of focus and spreads each point "
        "into a bigger circle; 0 leaves the picture unchanged.",
    )
    highlights: float = param(
        0.0,
        unit="ratio",
        min=0,
        max=50,
        doc="How much brighter pixels count in the average: a pixel of brightness b (0..1) counts 1 + highlights x b "
        "times as much as a black one, so with 0 every pixel counts the same (a plain average), and raising it turns "
        "bright lights into distinct, luminous discs (bokeh) while dark areas hardly bleed into them.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="light", params=LensBlurParams)
def lens_blur(rgb: np.ndarray, params: LensBlurParams) -> np.ndarray:
    """Blur the picture over a disc of `radius` px, as an out-of-focus lens does, favouring bright pixels.

    Every pixel becomes the average of the pixels within `radius` px of it (one on the rim counts for the share of it
    inside the disc, a linear ramp one pixel wide), each weighted by 1 + `highlights` x its brightness. Flat areas
    stay flat and the result never leaves the picture's own range. The cost hardly grows with the radius.
    """
    if params.radius == 0:
        return rgb.copy()
    reach = int(np.ceil(params.radius))
    offsets = np.arange(-reach, reach + 1)
    distance = np.hypot(offsets[:, np.newaxis], offsets[np.newaxis, :])
    # A pixel's weight is the share of it inside the disc, estimated by a linear ramp one pixel wide across the rim
    # (the usual anti-aliased edge: 1 half a pixel inside, 0 half a pixel outside, 1/2 on the rim itself).
    disc = np.clip(params.radius + 0.5 - distance, 0.0, 1.0)
    disc /= disc.sum()
    if params.highlights == 0:
        return convolve(rgb, disc, mode=params.edge)
    weight = 1.0 + params.highlights * luminance(rgb)
    weighted = convolve(rgb * weight[..., np.newaxis], disc, mode=params.edge)
    total = convolve(weight, disc, mode=params.edge)[..., np.newaxis]
    return np.clip(weighted / total, rgb.min(), rgb.max()).astype(np.float32)
