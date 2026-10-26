"""The `ambient_occlusion` effect: the picture darkened where its depth says it is sheltered, in creases and beside
nearer things."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur


class AmbientOcclusionParams(EffectParams):
    """An ambient occlusion: each pixel darkened by `gain` times how much farther it lies than the depth around it,
    within `radius`."""

    radius: float = param(
        10.0,
        unit="px",
        min=1,
        max=200,
        doc="The size of the neighbourhood that counts as shelter, the Gaussian sigma in px at the recipe's size over "
        "which the surrounding depth is averaged. Raising it darkens wider areas around nearer things; lowering it "
        "keeps the darkening to fine creases.",
    )
    gain: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=100,
        doc="How strongly a depth difference darkens: a pixel lying d farther than its surroundings (depth 0..1) is "
        "darkened by gain x d, at most to black. Raising it darkens shallow creases more and turns deep ones black "
        "sooner.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the averaging Gaussian is cut off, in sigmas: it reaches truncate x radius px. Larger is more "
        "exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the averaging sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge, so a "
        "tileable picture darkens without a seam).",
        advanced=True,
    )


@effect(category="light", params=AmbientOcclusionParams, opacity=0.6)
def ambient_occlusion(
    rgb: np.ndarray, params: AmbientOcclusionParams, *, depth: np.ndarray | None = None
) -> np.ndarray:
    """Darken the places the depth says are sheltered: creases, and the ground or wall right beside something nearer.

    Screen-space, from the depth alone: a pixel's cavity is the depth averaged by a Gaussian of sigma `radius` minus
    its own depth, positive where it lies farther than its surroundings. It is darkened by clip(gain x cavity, 0, 1),
    one grey factor on every channel, so the colour ratios are kept and nothing is brightened. Flat and evenly sloping
    depths are left as they are. A depth step at a subject's outline darkens its farther side (a contact shadow) and
    leaves the nearer side alone; raising `radius` widens that shadow. The step's `opacity` sets how much is mixed in.
    `depth` is filled in by the recipe from the depth model (never set it in a recipe file): float32 (h, w), 0..1,
    1 = nearest. Without a depth the picture is unchanged.
    """
    if depth is None:
        return rgb.copy()
    cavity = gaussian_blur(depth, params.radius, truncate=params.truncate, edge=params.edge) - depth
    occlusion = np.clip(params.gain * cavity, 0, 1)
    return (rgb * (1 - occlusion)[..., None]).astype(np.float32)
