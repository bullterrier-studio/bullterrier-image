"""The `drop_shadow` effect: copies of the subject's silhouette behind it, a soft shadow or a stack of paper layers."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.cutout import put_behind
from bullterrier_image.ops.shift import shift


class DropShadowParams(EffectParams):
    """The settings of `drop_shadow`: how many copies, where they fall, how soft they are, and their colours."""

    copies: int = param(
        1,
        unit="count",
        min=1,
        max=20,
        doc="How many copies of the silhouette are stacked behind the subject: 1 = a single shadow, more = a deeper "
        "stack of layers.",
    )
    offset: float = param(
        22.0,
        unit="px",
        min=0,
        max=1000,
        doc="How far each copy lies beyond the one before it, in px at the recipe's size: the first is `offset` from "
        "the subject, the last `copies` x `offset`. 0 = every copy hidden behind the subject, leaving only what "
        "`blur` spreads past its edge.",
    )
    angle: float = param(
        56.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        doc="The direction the copies step away in, clockwise as seen in the picture: 0 = to the right, 45 = to the "
        "lower right, 90 = down, 180 = to the left, -90 = up.",
    )
    blur: float = param(
        24.0,
        unit="px",
        min=0,
        max=200,
        doc="Gaussian sigma of each copy's soft edge in px: raising it makes the copies larger and fainter at their "
        "edge; 0 = hard-edged silhouettes.",
    )
    color: Color = param(
        "#000000",
        doc="The colour of the copy nearest the subject: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies "
        "behind the subject; another colour tints it.",
    )
    end_color: Color = param(
        "#000000",
        doc="The colour of the farthest copy; the copies between step evenly from `color` to it. With one copy only "
        "`color` is used.",
    )
    copy_opacity: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of what lies behind each copy it covers: 1 = fully, below 1 the copies show through each other "
        "and the picture; 0 = no copies. Overlapping copies compound (two copies at 0.5 cover 0.75); the step's "
        "`opacity` mixes the finished stack in once.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where each copy's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact "
        "and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the copies see beyond the picture's edge, when moved and softened: 'nearest' (the outermost row "
        "repeated, so a subject cut off by the border casts its copies on without a gap), 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="subject", params=DropShadowParams, opacity=0.6)
def drop_shadow(rgb: np.ndarray, params: DropShadowParams, *, subject: np.ndarray | None = None) -> np.ndarray:
    """Copies of the subject's silhouette behind it, each `offset` px farther along `angle` and softened by `blur`.

    One copy is a drop shadow. More, with `blur` 0 and a `color` to `end_color` ramp, are layers of cut paper. The
    copies run in colour from `color` (nearest the subject) to `end_color` (farthest); a nearer copy lies over the ones
    beyond it, each at `copy_opacity`, and the subject on top of all of them, so the subject itself is never changed.

    `subject` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject and so no copies: the picture comes back unchanged.
    """
    if subject is None:
        return rgb.copy()
    heading = np.radians(params.angle)
    step = params.offset * np.array([np.sin(heading), np.cos(heading)])  # (down, right) as seen in the picture
    near, far = np.asarray(params.color, np.float32), np.asarray(params.end_color, np.float32)
    along = np.arange(params.copies, dtype=np.float32) / max(params.copies - 1, 1)  # 0 at the first copy, 1 at the last
    colors = near + (far - near) * along[:, None]
    behind = rgb  # what lies behind the subject: the picture with the copies laid over it, the farthest first
    for number in range(params.copies, 0, -1):
        cover = shift(subject, *(step * number), edge=params.edge)
        cover = gaussian_blur(cover, params.blur, truncate=params.truncate, edge=params.edge)
        behind = behind + (params.copy_opacity * cover)[..., None] * (colors[number - 1] - behind)
    return put_behind(rgb, subject, 1, behind)  # constant: the copies' own opacity is already in `behind`
