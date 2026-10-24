"""The `echo` effect: translucent copies of the picture, each offset a step further, laid over it."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.shift import shift


class EchoParams(EffectParams):
    """The settings of `echo`: how many copies, the step between them, how fast they fade, and the edge rule."""

    copies: int = param(
        4,
        unit="count",
        min=0,
        max=32,
        doc="How many copies are laid over the picture, 0 to 32; 0 leaves the picture unchanged. More copies make a "
        "longer trail (the time grows with the count).",
    )
    dx: float = param(
        14.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far each copy lies to the right of the one before it, in pixels, -500 to 500 (negative is to the "
        "left); copy k is k times this far from the picture. A fraction of a pixel blends the two pixels it lies "
        "between.",
    )
    dy: float = param(
        0.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far each copy lies below the one before it, in pixels, -500 to 500 (negative is above); copy k is k "
        "times this far from the picture. With dx and dy both 0 the copies lie on the picture and change nothing.",
    )
    fade: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much weaker each copy is than the one before it, 0 to 1: the original counts 1, copy k counts (1 - "
        "fade) to the power k. 0 gives every copy the strength of the original; 0.5 halves each; 1 makes the copies "
        "vanish and leaves the picture unchanged.",
    )
    edge: Literal["nearest", "wrap", "reflect", "mirror"] = param(
        "nearest",
        doc="What a copy shows where it has moved off the picture's edge: 'nearest' repeats the picture's edge pixels "
        "(a smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the picture "
        "(the edge line repeated once), 'mirror' mirrors it about the edge line.",
    )


@effect(category="glitch", params=EchoParams)
def echo(rgb: np.ndarray, params: EchoParams) -> np.ndarray:
    """Lay `copies` translucent copies over the picture, each (`dx`, `dy`) px further from it and weaker by `fade`.

    The result is the weighted average of the picture (weight 1) and copy k, the picture moved by k times (`dx`,
    `dy`) (weight (1 - fade)^k), so the weights add up to one: the picture's brightness is kept, a flat picture stays
    flat and nothing leaves the range 0..1. What a copy shows where it moved off the picture is set by `edge`.
    """
    weights = (1 - params.fade) ** np.arange(params.copies + 1)  # of the picture (k = 0) and of each copy k
    total = rgb.copy()
    for k in range(1, params.copies + 1):
        total += float(weights[k]) * shift(rgb, k * params.dy, k * params.dx, edge=params.edge)
    return np.clip(total / weights.sum(), 0, 1).astype(np.float32)
