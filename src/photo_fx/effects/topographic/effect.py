"""The `topographic` effect: contour lines where the picture's brightness crosses an evenly spaced level."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.color import luminance
from photo_fx.ops.ramp import ramp


class TopographicParams(EffectParams):
    """A contour map of the picture's brightness: `levels` evenly spaced heights, lines drawn where it crosses them."""

    levels: int = param(
        8,
        unit="count",
        min=2,
        max=64,
        doc="How many bands the brightness range 0..1 is cut into; there is a contour line at each of the levels - 1 "
        "heights between them. 2 = a single line at middle grey, 8 = a clear contour map, 14 = a busy one, 64 = a "
        "dense engraving.",
    )
    thickness: float = param(
        1.5,
        unit="px",
        min=0.5,
        max=20,
        doc="How wide the lines are, in pixels across the line however steep the slope of brightness there; tuned "
        "for a picture about 480 px high. Edges are anti-aliased, so fractions of a pixel show as partial strength.",
    )
    line: Color = param(
        "#ffffff",
        doc="The colour of the contour lines: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    fill: Literal["none", "bands"] = param(
        "none",
        doc="What lies between the lines: 'none' (the photo itself, with the lines drawn over it) or 'bands' (the "
        "photo is replaced by flat colour, each band coloured from `colors` by its height, like a hypsometric map).",
    )
    colors: tuple[Color, ...] = param(
        ("#16324f", "#3f7d6a", "#d9c97a", "#9c6b43", "#f4f1ea"),
        doc="The colours of the bands for fill 'bands', from the lowest band (darkest brightness) to the highest, "
        "evenly spaced along the ramp; at least two. Ignored by fill 'none'.",
        min_length=2,
    )
    smoothing: float = param(
        5.0,
        unit="px",
        min=0,
        max=50,
        doc="Sigma in pixels of the Gaussian blur the brightness gets before the contours are found, so that grain "
        "and fine texture do not break the lines into specks; tuned for a picture about 480 px high. 0 = the "
        "brightness as it is, raising it gives smoother, simpler contours.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the smoothing blur is cut off, in sigmas: it reaches truncate x `smoothing` px. Larger is more "
        "exact and slower; below 3 the blur turns visibly boxy.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the smoothing sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="stylised", params=TopographicParams)
def topographic(rgb: np.ndarray, params: TopographicParams) -> np.ndarray:
    """Draw contour lines where the picture's smoothed brightness crosses `levels` evenly spaced heights.

    Brightness is the Rec. 709 luminance, blurred by `smoothing` px, scaled to h = luminance x `levels`; the contours
    are h = 1, 2, ..., `levels` - 1. A pixel's distance to the nearest contour in px is |h - k| divided by the slope
    of h there (central differences, halved on the outermost rows and columns, which have one neighbour: lines come
    out thinner there), so a line is `thickness` px wide however steep or shallow the brightness runs; a pixel at
    distance d is covered by clip(`thickness` / 2 + 1/2 - d, 0, 1) of the `line` colour. Where the brightness does
    not vary there is no contour. With fill 'bands' the photo is first replaced by the colour of its band
    floor(h) (the last band for white) taken from `colors`.
    """
    smooth = gaussian_blur(luminance(rgb), params.smoothing, truncate=params.truncate, edge=params.edge)
    height = smooth * params.levels
    # central differences; a border pixel's missing neighbour is itself, so there the slope is half the one-sided one
    down = np.diff(height, axis=0, prepend=height[:1], append=height[-1:])
    across = np.diff(height, axis=1, prepend=height[:, :1], append=height[:, -1:])
    slope = np.hypot((down[:-1] + down[1:]) / 2, (across[:, :-1] + across[:, 1:]) / 2)
    nearest = np.clip(np.round(height), 1, params.levels - 1)
    distance = np.divide(np.abs(height - nearest), slope, out=np.full_like(height, np.inf), where=slope > 0)
    cover = np.clip(params.thickness / 2 + 0.5 - distance, 0.0, 1.0)[..., None]
    if params.fill == "bands":
        band = np.clip(np.floor(height), 0, params.levels - 1)
        rgb = ramp((band / (params.levels - 1)).astype(np.float32), params.colors)
    line = np.asarray(params.line, np.float32)
    return (rgb + cover * (line - rgb)).astype(np.float32)
