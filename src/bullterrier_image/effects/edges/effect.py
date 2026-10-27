"""The `edges` effect: an edge map of the picture, drawn as a picture, by Sobel, Canny or difference of Gaussians."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.edges import connected_to, ridges, sobel_gradient

# constant: the second Gaussian of the difference is 1.6 times the first; Marr and Hildreth (1980) show that this ratio
# makes the difference of Gaussians closely match the Laplacian of a Gaussian
_DOG_RATIO = 1.6


class EdgesParams(EffectParams):
    """The settings of `edges`: how edges are found (`method`, `radius`), which ones count (`threshold`, `softness`)
    and how they are drawn (`color`, `background`)."""

    method: Literal["sobel", "canny", "dog"] = param(
        "canny",
        doc="How edges are found: 'sobel' (the steepness of the brightness: wide, soft bands that follow the picture's "
        "gradients), 'canny' (the same steepness reduced to thin connected lines: the weakest lines are dropped unless "
        "they join a strong one) or 'dog' (difference of Gaussians: only features between `radius` and 1.6 times "
        "`radius` in size, so slow gradients and fine texture both fall away).",
    )
    radius: float = param(
        1.5,
        unit="px",
        min=0.5,
        max=50,
        doc="How far the picture is smoothed before edges are looked for, as the Gaussian sigma in px at the recipe's "
        "size ('dog': the finer of its two Gaussians). Raising it drops fine detail and noise and makes broader, "
        "smoother edges.",
    )
    threshold: float = param(
        0.1,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How steep an edge must be to show, as a share of the steepest edge there can be after smoothing (black "
        "straight to white = 1; a faint edge is near 0.05). Raising it keeps only the strongest edges.",
    )
    softness: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How gradually edges fade in around `threshold`: they are fully drawn from threshold x (1 + softness) and "
        "not at all below threshold x (1 - softness), and 'canny' also keeps a line down to that lower value if it "
        "touches one above the upper. 0 = every edge fully drawn or not at all; raising it adds weaker edges as "
        "faint lines.",
    )
    color: Color = param(
        "#ffffff",
        doc="The colour of the edges: '#rrggbb', '#rgb' or [r, g, b] 0..1. Laid over a picture with the blend mode "
        "'screen' only the edges light up; dark on a light `background` it is line art.",
    )
    background: Color = param(
        "#000000",
        doc="The colour where there is no edge: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black suits 'screen' blending, "
        "a light colour with a dark `color` gives a drawing.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the Gaussians are cut off, in sigmas: they reach truncate x radius px. Larger is more exact and "
        "slower.",
        advanced=True,
    )
    edge: Edge = param(
        "reflect",
        doc="What the smoothing and the gradient see beyond the picture's border: 'reflect' (mirrored, edge pixel "
        "repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the "
        "opposite edge).",
        advanced=True,
    )


def _gradient(
    grey: np.ndarray, method: str, radius: float, truncate: float, edge: Edge
) -> tuple[np.ndarray, np.ndarray]:
    """The gradient the method looks at: the picture's own, smoothed by `radius`, or for 'dog' that of the difference
    of two Gaussians."""
    if method == "dog":
        band = gaussian_blur(grey, radius, truncate=truncate, edge=edge)
        band -= gaussian_blur(grey, radius * _DOG_RATIO, truncate=truncate, edge=edge)
        return sobel_gradient(band, edge=edge)
    return sobel_gradient(gaussian_blur(grey, radius, truncate=truncate, edge=edge), edge=edge)


def _steepest(params: EdgesParams) -> float:
    """The strength of the steepest edge the method can see at `radius`: a step from black straight to white, found by
    running the method on one, wide enough that its border cannot touch the step."""
    reach = int(params.truncate * params.radius * _DOG_RATIO + 0.5) + 3  # constant: 3 px of margin beyond the smoothing
    step = np.zeros((3, 2 * reach), np.float32)
    step[:, reach:] = 1.0  # constant: black to white, the full range of a picture
    return float(np.hypot(*_gradient(step, params.method, params.radius, params.truncate, "nearest")).max())


@effect(category="stylised", params=EdgesParams)
def edges(rgb: np.ndarray, params: EdgesParams) -> np.ndarray:
    """Edge detection and line art: outlines in `color` on `background`, by Sobel, Canny or difference of Gaussians.

    The picture's brightness is smoothed by `radius`, and its steepness (a black-to-white step reads 1, whichever
    method and radius) decides how fully an edge is drawn: not at all below `threshold` x (1 - `softness`), fully from
    threshold x (1 + softness), faded between. 'canny' keeps only the crest of each edge, one or two pixels wide, and
    drops the weak ones that join no strong one; 'dog' looks at the steepness of the difference between two Gaussians
    (`radius` and 1.6 times that), so it outlines features of about that size.
    Flat areas are always `background`. Screened over the picture (a step's `blend_mode = "screen"`) it lights up
    the edges; a dark `color` on a light `background` is line art.
    """
    p = params
    gx, gy = _gradient(luminance(rgb), p.method, p.radius, p.truncate, p.edge)
    steepness = np.hypot(gx, gy) / _steepest(p)
    lower, upper = p.threshold * (1 - p.softness), p.threshold * (1 + p.softness)
    if upper > lower:
        drawn = np.clip((steepness - lower) / (upper - lower), 0.0, 1.0)
    else:  # softness 0: a hard threshold
        drawn = (steepness > upper).astype(np.float32)
    if p.method == "canny":
        crests = ridges(steepness, gx, gy) & (steepness > lower)
        drawn = drawn * connected_to(crests, crests & (steepness > upper))
    background = np.asarray(p.background, np.float32)
    color = np.asarray(p.color, np.float32)
    return (background + (color - background) * drawn[..., None]).astype(np.float32)
