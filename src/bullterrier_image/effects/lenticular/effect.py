"""The `lenticular` effect: vertical strips, every other one slid sideways, like a lenticular print seen off-centre."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import shift


class LenticularParams(EffectParams):
    """A lenticular interlace: the picture cut into vertical strips, every second one shown slid sideways."""

    strip: int = param(
        6,
        unit="px",
        min=1,
        max=500,
        doc="The width of each vertical strip in pixels, counted from the left edge; a last strip at the right edge "
        "may be narrower. Narrow strips make a fine comb, wide ones a coarse stagger; a strip wider than the picture "
        "leaves it whole. Tuned for a picture about 480 px high.",
    )
    shift: float = param(
        5.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far every second strip (the 2nd, 4th, ... from the left) is slid sideways, in pixels: positive moves "
        "its picture right, negative left, 0 leaves the picture unchanged. A distance that is not a whole number "
        "blends the two pixels it lies between. Raising it separates the two interleaved views further.",
    )
    edge: Literal["wrap", "nearest", "reflect", "mirror"] = param(
        "nearest",
        doc="What a slid strip shows where its picture has moved off the picture's side: 'nearest' (the outermost "
        "column repeated), 'wrap' (the opposite side comes round), 'reflect' (mirrored, edge column repeated), "
        "'mirror' (mirrored about the edge column).",
        advanced=True,
    )


@effect(category="distortion", params=LenticularParams)
def lenticular(rgb: np.ndarray, params: LenticularParams) -> np.ndarray:
    """Cut the picture into vertical strips of `strip` px and slide every second one sideways by `shift` px.

    The strips keep their places; what changes is what they show: the 1st, 3rd, ... strips show the picture as it is
    and the 2nd, 4th, ... show the whole picture moved sideways by `shift`, so the two views are interlaced as under a
    lenticular lens. A strip shows the moved picture at the strip's own columns.
    """
    moved = shift(rgb, 0, params.shift, edge=params.edge)  # constant: a strip slides sideways only, no move down
    slid = (np.arange(rgb.shape[1]) // params.strip) % 2 == 1
    return np.where(slid[None, :, None], moved, rgb)
