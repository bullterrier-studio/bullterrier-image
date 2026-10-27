"""The `drop_shadow` effect: copies of the subject's silhouette behind it: a shadow, paper layers, a long shadow."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.cutout import put_behind
from bullterrier_image.ops.shift import BoxedImage


class DropShadowParams(EffectParams):
    """The settings of `drop_shadow`: how many copies, where they fall, how soft they are, and their colours."""

    copies: int = param(
        1,
        unit="count",
        min=1,
        max=400,
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
    fade: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of `copy_opacity` the farthest copy loses, falling in a straight line along the copies: 0 = all "
        "copies alike, 0.5 = the farthest half as opaque, 1 = it vanishes. With one copy nothing fades.",
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
    """Copies of the subject's silhouette behind it, each `offset` px farther along `angle` and softened by `blur`:
    a drop shadow, layered paper cutouts or a long shadow.

    One copy is a drop shadow. More, with `blur` 0 and a `color` to `end_color` ramp, are layers of cut paper; many
    at a small `offset` in one colour with `fade` are a long shadow (preset `long_shadow`). The copies run in colour
    from `color` (nearest the subject) to `end_color` (farthest); a nearer copy lies over the ones beyond it, each at
    `copy_opacity` falling by `fade`, and the subject on top of all of them, so the subject itself is never changed.

    `subject` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject and so no copies: the picture comes back unchanged.
    """
    if subject is None:
        return rgb.copy()
    heading = np.radians(params.angle)
    step = params.offset * np.array([np.sin(heading), np.cos(heading)])  # (down, right) as seen in the picture
    near, far = np.asarray(params.color, np.float32), np.asarray(params.end_color, np.float32)
    along = np.arange(params.copies, dtype=np.float32) / max(params.copies - 1, 1)  # 0 at the first copy, 1 at the last
    opacity = params.copy_opacity * (1 - params.fade * along)
    # Laying the copies over the picture, the farthest first, is the same as walking them from the nearest: each takes
    # its opacity of what the nearer ones left clear, and its colour, which is `near` plus `along` of the way to `far`,
    # is summed as `near` and the `along`-weighted share, so every copy costs one-channel arrays, not three.
    # Every copy is a shift of the one silhouette, which is zero outside a box: it is read and laid on only where the
    # copy is not zero (the whole picture, once softened).
    silhouette = BoxedImage(subject, edge=params.edge)
    clear = np.ones(subject.shape, np.float32)
    along_share = np.zeros(subject.shape, np.float32)
    for number in range(1, params.copies + 1):
        rows, cols, cover = silhouette.shifted(*(step * number))
        if params.blur > 0:
            whole = np.zeros(subject.shape, np.float32)
            whole[rows, cols] = cover
            cover = gaussian_blur(whole, params.blur, truncate=params.truncate, edge=params.edge)
            rows = cols = slice(None)
        taken = clear[rows, cols] * (opacity[number - 1] * cover)
        clear[rows, cols] -= taken
        along_share[rows, cols] += taken * along[number - 1]
    behind = rgb * clear[..., None] + near * (1 - clear)[..., None] + (far - near) * along_share[..., None]
    return put_behind(rgb, subject, 1, behind)  # constant: the copies' own opacity is already in `behind`
