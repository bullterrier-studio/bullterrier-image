"""The `scan_roll` effect: the picture rolled vertically or horizontally, like a video sync that slipped."""

import math
from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


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
    )


@effect(category="glitch", params=ScanRollParams)
def scan_roll(rgb: np.ndarray, params: ScanRollParams) -> np.ndarray:
    """Roll the picture by `offset` of its height (`axis` 'vertical') or width ('horizontal'), as a slipped sync does.

    Every line moves by the same distance, by whole or fractional pixels (a fraction blends the two lines it lies
    between). What comes into view where the picture left is set by `edge`: with 'wrap' it is the part that left on the
    other side, so nothing is lost and a whole-picture roll is the picture itself.
    """
    # the array axis that rolls: the rows (0) for 'vertical', the columns (1) for 'horizontal'
    dimension = int(params.axis == "horizontal")
    size = rgb.shape[dimension]
    distance = params.offset * size  # px; positive moves the picture toward higher rows or columns
    whole = math.floor(distance)
    fraction = distance - whole
    lines = np.arange(size)

    def source(index: np.ndarray) -> np.ndarray:
        """The line of the picture each position of `index` shows, by the rule for beyond its edge."""
        if params.edge == "wrap":
            return index % size
        if params.edge == "nearest":
            return np.clip(index, 0, size - 1)
        period = 2 * size if params.edge == "reflect" else max(2 * size - 2, 1)
        folded = index % period
        return np.where(folded >= size, period - (folded + 1 if params.edge == "reflect" else folded), folded)

    # the output at line i is the picture at i - distance, which lies between lines i - whole - 1 and i - whole
    rolled = np.take(rgb, source(lines - whole), axis=dimension)
    if fraction == 0:
        return rolled
    behind = np.take(rgb, source(lines - whole - 1), axis=dimension)
    return (rolled * (1 - fraction) + behind * fraction).astype(np.float32)
