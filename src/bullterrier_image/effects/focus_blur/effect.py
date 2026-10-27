"""The `focus_blur` effect: the picture blurred by how far a field's value lies from the one kept in focus."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import graded_blur


class FocusBlurParams(EffectParams):
    """A focus: pixels whose `field` value lies near `focus` stay sharp, and the farther a value lies from it, higher
    or lower, the more the pixel is blurred, up to `radius`."""

    field: FieldName = param(
        "depth",
        doc="The selection the focus is measured on: `depth` (1 = nearest: a lens focused in the scene), `light`, "
        "`subject`, or a region of the recipe such as a linear gradient (a band of focus across the picture, "
        "the miniature look) or a radial one. Its value at a pixel is compared with `focus`.",
    )
    focus: float = param(
        0.8,
        unit="fraction",
        min=0,
        max=1,
        doc="The value of the field that stays sharp, as a fraction of its range: for `depth` 1 = the nearest and "
        "0 = the farthest. Raising it moves the focus towards the field's high values; driving it moves the focus "
        "through the scene.",
    )
    width: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="How wide a band of the field's values around the focus stays exactly as it is, as a fraction of its "
        "range. Raising it keeps more of the picture sharp.",
    )
    feather: float = param(
        0.4,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How far in the field's values beyond the band's edge the blur takes to reach its full radius, as a "
        "fraction of its range: the blur grows evenly across it. Raising it makes a gentler fall-off, lowering it an "
        "abrupt one.",
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


@effect(category="light", params=FocusBlurParams)
def focus_blur(rgb: np.ndarray, params: FocusBlurParams, *, field: np.ndarray) -> np.ndarray:
    """A shallow depth of field, or a tilt-shift miniature: each pixel is blurred by w x `radius`,
    w = clip((|field - focus| - width / 2) / feather, 0, 1), so the pixels whose field lies within `width` / 2 of
    `focus` are returned exactly as they are.

    The blur is `levels` copies of the picture with sigma radius x k / levels, and a pixel mixes the two copies (the
    sharp picture being copy 0) either side of w x `levels`. `field` is the selection the recipe names (`depth`, a
    linear gradient...), float32 (h, w), 0..1: `depth` gives a lens focused in the scene, a linear gradient a sharp
    band across the picture.
    """
    weight = np.clip((np.abs(field - params.focus) - params.width / 2) / params.feather, 0.0, 1.0)
    out = graded_blur(rgb, weight, params.radius, levels=params.levels, truncate=params.truncate, edge=params.edge)
    return np.clip(out, 0.0, 1.0).astype(np.float32)
