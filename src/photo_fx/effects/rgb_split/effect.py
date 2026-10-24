"""The `rgb_split` effect: the red and blue channels pushed apart along an angle, green left in place."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.shift import shift


class RgbSplitParams(EffectParams):
    """The settings of `rgb_split`: the direction and size of the split, and the edge rule."""

    angle: float = param(
        30.0,
        unit="degrees",
        min=0,
        max=360,
        doc="The direction red moves, in degrees counter-clockwise from pointing right (as on a protractor), 0 to 360: "
        "0 = right, 90 = up, 180 = left, 270 = down. Blue moves the opposite way.",
    )
    amount: float = param(
        8.0,
        unit="px",
        min=0,
        max=500,
        doc="How far red and blue each move from where they were, in pixels, 0 to 500; they end up twice this far "
        "apart. 0 leaves the picture unchanged. A fraction of a pixel blends the two pixels it lies between. Raising "
        "it pulls the colour fringes further apart.",
    )
    edge: Literal["nearest", "wrap", "reflect", "mirror"] = param(
        "nearest",
        doc="What a moved channel shows where it has left the picture's edge: 'nearest' repeats the channel's edge "
        "pixels (a coloured smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors "
        "the channel (the edge line repeated once), 'mirror' mirrors it about the edge line.",
        advanced=True,
    )


@effect(category="glitch", params=RgbSplitParams)
def rgb_split(rgb: np.ndarray, params: RgbSplitParams) -> np.ndarray:
    """Push the red and blue channels `amount` px apart along `angle`, leaving green where it is.

    Red moves `amount` px in the direction `angle` (degrees counter-clockwise from pointing right: 0 right, 90 up),
    blue moves `amount` px the opposite way, green stays: on a grey picture the three channels end up exactly `amount`
    px from each other in a row, which shows as a red fringe on one side of an edge and a blue one on the other. Whole
    or fractional distances are allowed; what a channel shows where it has left the picture is set by `edge`.
    """
    radians = np.deg2rad(params.angle)
    dx = params.amount * np.cos(radians)
    dy = -params.amount * np.sin(radians)  # rows count down, so a positive angle (up) moves toward fewer rows
    red = shift(rgb[..., 0], dy, dx, edge=params.edge)
    blue = shift(rgb[..., 2], -dy, -dx, edge=params.edge)
    return np.stack([red, rgb[..., 1], blue], axis=-1)
