"""The `scan_roll` effect: the picture rolled vertically or horizontally, like a video sync that slipped."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import shift


class ScanRollParams(EffectParams):
    """The settings of `scan_roll`: the direction, how far, and what fills the gap."""

    axis: Literal["vertical", "horizontal"] = param(
        "vertical",
        doc="The direction of the roll: 'vertical' slides the picture up or down, as a television whose vertical hold "
        "slipped; 'horizontal' slides it left or right, as a horizontal hold would.",
    )
    offset: float = param(
        0.2,
        unit="fraction",
        min=-1,
        max=1,
        doc="How far the picture rolls, as a fraction of its height ('vertical') or width ('horizontal'), -1 to 1: "
        "positive moves it down or to the right, negative up or to the left, 0 leaves it where it is. A whole picture "
        "(1 or -1) with 'wrap' brings it back to where it was. A distance that ends between two pixels blends the two "
        "(linear interpolation along the roll).",
    )
    edge: Literal["wrap", "nearest", "reflect", "mirror"] = param(
        "wrap",
        doc="What fills the gap the roll opens: 'wrap' brings back what left on the other side, a true roll; "
        "'nearest' holds the picture's first or last line there (a smear); 'reflect' mirrors the picture (the edge "
        "line repeated once); 'mirror' mirrors it about the edge line.",
        advanced=True,
    )


@effect(category="glitch", params=ScanRollParams)
def scan_roll(rgb: np.ndarray, params: ScanRollParams) -> np.ndarray:
    """Roll the picture by `offset` of its height (`axis` 'vertical') or width ('horizontal'), as a slipped sync does.

    Every line moves by the same distance, by whole or fractional pixels (a fraction blends the two lines it lies
    between). What comes into view where the picture left is set by `edge`: with 'wrap' it is the part that left on the
    other side, so nothing is lost and a whole-picture roll is the picture itself.
    """
    horizontal = params.axis == "horizontal"
    distance = (
        params.offset * rgb.shape[horizontal]
    )  # px, along the rows (0) or the columns (1); positive is down or right
    return shift(rgb, distance * (not horizontal), distance * horizontal, edge=params.edge)
