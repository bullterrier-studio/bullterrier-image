"""The `reflection` effect: the picture mirrored in a floor under the subject, fading with distance."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur


class ReflectionParams(EffectParams):
    """The settings of `reflection`: how tall the reflection is, how it fades and how soft and strong it is."""

    height: float = param(
        0.4,
        unit="fraction",
        min=0,
        max=1,
        doc="How tall the reflection is, as a fraction of the subject's height: 0.4 = 40% of it is mirrored into the "
        "floor below the subject's lowest row, 1 = all of it; 0 = no reflection. Where the picture ends first, the "
        "reflection is cut there.",
    )
    fade: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of its strength the reflection loses between the floor line and its end, in a straight line: "
        "0 = as strong at the end as at the floor line, 1 = fades out completely at its end.",
    )
    blur: float = param(
        2.0,
        unit="px",
        min=0,
        max=100,
        doc="Gaussian sigma of the reflection's softness in px at the recipe's size: raising it makes a blurrier "
        "mirror, as on a rough floor; 0 = a sharp mirror image.",
    )
    opacity: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the floor the reflection replaces at the floor line, where it is strongest: 0 = no "
        "reflection, 1 = a perfect mirror.",
    )
    threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of a matte row, 0 to 1, must be subject for the row to count as part of it when its lowest and "
        "highest rows are found: 0.5 = a pixel of at least half. Lowering it counts faint fringes, so the floor "
        "line moves down and the subject's height grows.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the blur's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge, and beyond the floor line, where the reflection is "
        "mirrored: 'reflect' (mirrored, edge pixel repeated, which continues the mirror image), 'mirror' (mirrored "
        "about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="subject", params=ReflectionParams)
def reflection(rgb: np.ndarray, params: ReflectionParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """A floor mirror: the picture above the subject's lowest row is mirrored about it, softened and fading with depth.

    The floor line is the bottom of the subject; the reflection is `height` of the subject's height deep, blurred by
    `blur`, and mixed into the floor below by `opacity` at the floor line and by less and less down to `fade` of that
    at its end. It stops at the picture's bottom edge, and the picture above the floor line is never changed.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one, or
    when nothing in it counts as the subject (see `threshold`), there is no floor line and so no reflection: the
    picture comes back unchanged.
    """
    if matte is None:
        return rgb.copy()
    subject_rows = np.flatnonzero(matte.max(axis=1) >= params.threshold)
    if subject_rows.size == 0:
        return rgb.copy()
    floor = subject_rows[-1] + 1  # the first row below the subject
    reach = round(params.height * (floor - subject_rows[0]))  # rows the reflection would have, with room enough
    depth = min(reach, rgb.shape[0] - floor)  # rows it has: it is cut where the picture ends
    mirror = gaussian_blur(rgb[:floor], params.blur, truncate=params.truncate, edge=params.edge)
    mirrored = mirror[floor - depth : floor][::-1]  # the row just above the floor line comes first
    strength = (params.opacity * (1.0 - params.fade * np.arange(depth, dtype=np.float32) / reach))[:, None, None]
    out = rgb.copy()
    out[floor : floor + depth] = (1.0 - strength) * rgb[floor : floor + depth] + strength * mirrored
    return out
