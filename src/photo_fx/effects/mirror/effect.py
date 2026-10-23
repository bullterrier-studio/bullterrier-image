"""The `mirror` effect: one half of the picture kept, the other replaced by its reflection."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class MirrorParams(EffectParams):
    """The settings of `mirror`: which mirror lines, and which side of each is kept."""

    axis: Literal["vertical", "horizontal", "both"] = param(
        "vertical",
        doc="The mirror line, named for the line itself: 'vertical' puts a vertical line down the middle, so the two "
        "sides are each other's reflection (left against right, as a face made symmetric); 'horizontal' puts a "
        "horizontal line across the middle (top against bottom, as a reflection in water); 'both' does both, so the "
        "picture is symmetric in four quarters.",
    )
    keep: Literal["first", "last"] = param(
        "first",
        doc="Which side of each mirror line stays as it is: 'first' keeps the left half (and, for 'horizontal' or "
        "'both', the top half), 'last' keeps the right half (and the bottom half). The other side is replaced by the "
        "reflection of the kept one. On a picture with an odd width or height the middle column or row lies on the "
        "line and stays, whichever side is kept.",
    )


@effect(category="distortion", params=MirrorParams)
def mirror(rgb: np.ndarray, params: MirrorParams) -> np.ndarray:
    """Make the picture symmetric: keep one half and replace the other by its reflection.

    The mirror line is a vertical line down the middle (`axis` 'vertical'), a horizontal one across it ('horizontal')
    or both ('both'). The kept side (`keep`) is copied untouched, pixel for pixel, so the result is exactly
    symmetric and uses only the picture's own values.
    """
    out = rgb
    for dimension, wanted in enumerate((("horizontal", "both"), ("vertical", "both"))):  # rows, then columns
        if params.axis not in wanted:
            continue
        size = rgb.shape[dimension]
        position = np.arange(size)
        # each position reads the kept side: the smaller of itself and its reflection keeps the first half, the
        # larger the last
        reflection = size - 1 - position
        source = np.minimum(position, reflection) if params.keep == "first" else np.maximum(position, reflection)
        out = np.take(out, source, axis=dimension)
    return out.copy() if out is rgb else out
