"""The `layered_cutouts` effect: offset copies of the subject's silhouette stacked behind it, like layers of paper."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.cutout import put_behind
from photo_fx.ops.shift import shift


class LayeredCutoutsParams(EffectParams):
    """The settings of `layered_cutouts`: how many layers, how far apart and in which direction, and their colours."""

    layers: int = param(
        4,
        unit="count",
        min=1,
        max=20,
        doc="How many copies of the silhouette are stacked behind the subject. Raising it makes a deeper stack.",
    )
    offset: float = param(
        14.0,
        unit="px",
        min=0,
        max=200,
        doc="How far each layer lies beyond the one before it, in px at the recipe's size: the first is `offset` "
        "from the subject, the last `layers` x `offset`. 0 = every layer hidden behind the subject.",
    )
    angle: float = param(
        45.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="The direction the layers step away in, clockwise as seen in the picture: 0 = to the right, 45 = to the "
        "lower right, 90 = down, 180 = to the left, -90 = up.",
    )
    color: Color = param(
        "#ff8a5b",
        doc="The colour of the layer nearest the subject: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    end_color: Color = param(
        "#3b1f6b",
        doc="The colour of the farthest layer; the layers between step evenly from `color` to it. With one layer "
        "only `color` is used.",
    )
    opacity: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of what lies behind each layer it covers: 1 = fully (paper), below 1 the layers show through "
        "each other and the picture; 0 = no layers.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the layers see beyond the picture's edge when the silhouette is moved: 'nearest' (the outermost "
        "row repeated, so a subject cut off by the border has layers that go on past it), 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="subject", params=LayeredCutoutsParams)
def layered_cutouts(rgb: np.ndarray, params: LayeredCutoutsParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """Copies of the subject's silhouette stacked behind it, each `offset` px farther along `angle`, like cut paper.

    The layers run in colour from `color` (nearest the subject) to `end_color` (farthest); a nearer layer lies in
    front of the ones beyond it, and the subject on top of all of them, so the subject itself is never changed.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette. Without one there is
    no subject and so no layers: the picture comes back unchanged.
    """
    if matte is None:
        return rgb.copy()
    heading = np.radians(params.angle)
    step = params.offset * np.array([np.sin(heading), np.cos(heading)])  # (down, right) as seen in the picture
    near, far = np.asarray(params.color, np.float32), np.asarray(params.end_color, np.float32)
    along = np.arange(params.layers, dtype=np.float32) / max(
        params.layers - 1, 1
    )  # 0 at the first layer, 1 at the last
    colors = near + (far - near) * along[:, None]
    behind = rgb  # what lies behind the subject: the picture with the layers laid over it, the farthest first
    for number in range(params.layers, 0, -1):
        cover = params.opacity * shift(matte, *(step * number), edge=params.edge)
        behind = behind + cover[..., None] * (colors[number - 1] - behind)
    return put_behind(rgb, matte, 1, behind)  # constant: the layers' own opacity is already in `behind`
