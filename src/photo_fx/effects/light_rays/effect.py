"""The `light_rays` effect: god rays, the highlights streaked away from a point of light."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import highlights, screen
from photo_fx.ops.radial import accumulate


class LightRaysParams(EffectParams):
    """God rays: the highlights are streaked away from the point (`x`, `y`) in `samples` steps that fade by `decay`,
    and screened back over the picture."""

    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last. The rays point away from it.",
    )
    y: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = "
        "the middle, 1 = the last. The rays point away from it.",
    )
    length: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the rays reach back toward the light, as a fraction of each pixel's distance from it: a pixel "
        "gathers light from the line between itself and the light, as far as this share of the way (0.5 = half way, "
        "1 = all the way to the light). A highlight streaks outward from itself up to 1 / (1 - length) times its "
        "distance from the light; 0 gives no rays.",
    )
    threshold: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1, per channel) where light starts: only values above it make rays, rising to full "
        "at 1. Raising it limits the rays to the brightest highlights.",
    )
    amount: float = param(
        2.5,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the rays are screened over the picture: 0 = off, 1 = the rays at full brightness; above 1 "
        "they saturate to white sooner. The rays are an average along the pixel's line to the light, so a highlight "
        "far from the pixel gives them much less than the highlight itself: a few times 1 is usual.",
    )
    samples: int = param(
        16,
        unit="count",
        min=1,
        max=256,
        doc="How many readings of the picture along each ray are added up, spread evenly over its length: more is "
        "smoother and slower (the time grows with it); few show as separate ghost copies of a highlight. With 1 the "
        "highlights only brighten themselves.",
    )
    decay: float = param(
        0.95,
        unit="ratio",
        min=0,
        max=1,
        doc="How much less each reading counts than the one nearer the pixel (the weights are decay^0, decay^1... "
        "scaled to add up to 1): 1 = all alike, rays of even brightness to their tips; lower values make them fade "
        "away from the highlight; 0 = no rays at all.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, but "
        "steps and ghosts show), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits "
        "a cubic spline through the neighbouring pixels (sharpest, a little slower).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a reading sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). The "
        "readings lie between a pixel and the light, so they leave the picture only when the light itself is "
        "outside it; this rule then matters at the border for 'bicubic' only, whose spline fit reads it.",
    )


@effect(category="light", params=LightRaysParams)
def light_rays(rgb: np.ndarray, params: LightRaysParams) -> np.ndarray:
    """God rays: what is brighter than `threshold` is streaked away from (`x`, `y`) and screened over the picture by
    `amount`.

    Every pixel gathers the highlights from `samples` points on the line between itself and the light, from itself
    to `length` of the way there, each reading `decay` times as strong as the one before and the weights adding up to
    1. A highlight streaks outward from the light, up to 1 / (1 - length) times its distance from it. The picture
    only ever gets brighter, and black stays black.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    weights = params.decay ** np.arange(params.samples)
    weights /= weights.sum()  # the rays are an average, so a picture bright all over stays as bright as its highlights
    rays = accumulate(
        highlights(rgb, params.threshold),
        centre=(params.y * (height - 1), params.x * (width - 1)),  # (row, column) of the light, in pixels
        kind="zoom",
        offsets=-params.length * np.arange(params.samples) / params.samples,  # scale 1 down to 1 - length: toward it
        weights=[float(weight) for weight in weights],
        interpolation=params.interpolation,
        edge=params.edge,
    )
    return screen(rgb, params.amount * rays)
