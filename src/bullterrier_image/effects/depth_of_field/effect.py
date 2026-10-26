"""The `depth_of_field` effect: a lens focused at one depth, the picture blurred by its distance from it."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import graded_blur


class DepthOfFieldParams(EffectParams):
    """A depth of field: depths near `focus` stay sharp, and the farther a pixel's depth lies from it, nearer or
    farther, the more it is blurred, up to `radius`."""

    focus: float = param(
        0.8,
        unit="fraction",
        min=0,
        max=1,
        doc="The depth that stays sharp, as a fraction of the picture's depth range: 1 = the nearest, 0 = the "
        "farthest. Raising it moves the focus towards the camera.",
    )
    width: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="How wide a band of depth around the focus stays exactly as it is, as a fraction of the depth range. "
        "Raising it keeps more of the picture sharp.",
    )
    feather: float = param(
        0.4,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How far in depth beyond the band's edge the blur takes to reach its full radius, as a fraction of the "
        "depth range: the blur grows evenly across it. Raising it makes a gentler fall-off, lowering it an abrupt one.",
    )
    radius: float = param(
        10.0,
        unit="px",
        min=0,
        max=100,
        doc="The Gaussian sigma in px at the recipe's size that the blur reaches at the full distance from the focus "
        "(a further `feather` from the band's edge): larger blurs the out-of-focus picture more; 0 = no blur.",
    )
    levels: int = param(
        6,
        unit="count",
        min=1,
        max=16,
        doc="How many blurred copies of the picture the blur is built from: copy k has the sigma radius x k / "
        "levels, and each pixel mixes the two copies (or the sharp picture and the first) nearest its distance "
        "from the focus. More is smoother between levels and slower; 1 mixes the sharp picture with one blur.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where each blurred copy's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is "
        "more exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=DepthOfFieldParams)
def depth_of_field(rgb: np.ndarray, params: DepthOfFieldParams, *, depth: np.ndarray | None = None) -> np.ndarray:
    """A shallow depth of field: each pixel is blurred by w x `radius`, w = clip((|depth - focus| - width / 2) /
    feather, 0, 1), so the depths within `width` / 2 of `focus` are returned exactly as they are.

    The blur is `levels` copies of the picture with sigma radius x k / levels, and a pixel mixes the two copies (the
    sharp picture being copy 0) either side of w x `levels`. `depth` is filled in by the recipe from the depth model
    (never set it in a recipe file): float32 (h, w), 0..1, 1 = nearest. Without a depth the picture is unchanged.
    """
    if depth is None:
        return rgb.copy()
    weight = np.clip((np.abs(depth - params.focus) - params.width / 2) / params.feather, 0.0, 1.0)
    out = graded_blur(rgb, weight, params.radius, levels=params.levels, truncate=params.truncate, edge=params.edge)
    return np.clip(out, 0.0, 1.0).astype(np.float32)
