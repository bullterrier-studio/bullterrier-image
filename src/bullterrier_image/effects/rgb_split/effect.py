"""The `rgb_split` effect: red pushed one way along an angle, blue (and for a red-cyan pair green) the other way."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import shift


class RgbSplitParams(EffectParams):
    """The settings of `rgb_split`: which channels part, the direction and size of the split, and the edge rule."""

    pair: Literal["red_blue", "red_cyan"] = param(
        "red_blue",
        doc="Which channels part: 'red_blue' moves red one way and blue the other and leaves green in place (colour "
        "fringes on a colour picture); 'red_cyan' moves red one way and green and blue together the other, as in 3D "
        "glasses (grey the picture first, e.g. with `grade` at saturation 0, for the classic anaglyph).",
    )
    angle: float = param(
        330.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction red moves, in degrees clockwise from pointing right on the screen (as CSS `rotate`), 0 to "
        "360: 0 = right, 90 = down, 180 = left, 270 = up. Blue (and green with 'red_cyan') moves the opposite way.",
    )
    amount: float = param(
        8.0,
        unit="px",
        min=0,
        max=500,
        doc="How far red and its opposite channels each move from where they were, in pixels, 0 to 500; they end up "
        "twice this far apart. 0 leaves the picture unchanged. A fraction of a pixel blends the two pixels it lies "
        "between. Raising it pulls the colour fringes further apart.",
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
    """Push red `amount` px along `angle` and the opposite channels `amount` px the other way.

    Red moves `amount` px in the direction `angle` (degrees clockwise from pointing right: 0 right, 90 down); blue moves
    `amount` px the opposite way. Green stays with 'red_blue' and moves with blue with 'red_cyan'. On a grey picture the
    channels end up exactly `amount` px from each other in a row, which shows as a red fringe on one side of an edge and
    a blue (or cyan) one on the other. Whole or fractional distances are allowed; what a channel shows where it has
    left the picture is set by `edge`.
    """
    radians = np.deg2rad(params.angle)
    dx = params.amount * np.cos(radians)
    dy = params.amount * np.sin(radians)  # rows count down, so a positive angle (clockwise) moves toward more rows
    red = shift(rgb[..., 0], dy, dx, edge=params.edge)
    blue = shift(rgb[..., 2], -dy, -dx, edge=params.edge)
    green = shift(rgb[..., 1], -dy, -dx, edge=params.edge) if params.pair == "red_cyan" else rgb[..., 1]
    return np.stack([red, green, blue], axis=-1)
