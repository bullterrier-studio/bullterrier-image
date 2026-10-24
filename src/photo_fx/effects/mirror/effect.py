"""The `mirror` effect: one half of the picture kept, the other replaced by its reflection."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect

# direction -> (the half of the rows that is kept, the half of the columns that is kept); None = that line is not used
_KEPT: dict[str, tuple[str | None, str | None]] = {
    "left_to_right": (None, "first"),
    "right_to_left": (None, "last"),
    "top_to_bottom": ("first", None),
    "bottom_to_top": ("last", None),
    "top_left": ("first", "first"),
    "top_right": ("first", "last"),
    "bottom_left": ("last", "first"),
    "bottom_right": ("last", "last"),
}


class MirrorParams(EffectParams):
    """The settings of `mirror`: which part of the picture is copied onto the rest."""

    direction: Literal[
        "left_to_right",
        "right_to_left",
        "top_to_bottom",
        "bottom_to_top",
        "top_left",
        "top_right",
        "bottom_left",
        "bottom_right",
    ] = param(
        "left_to_right",
        doc="Which part is kept as it is and copied, reflected, onto the rest, named as image editors name it. "
        "'left_to_right' copies the left half onto the right (a face made symmetric about a vertical line down "
        "the middle), 'right_to_left' the right half onto the left, 'top_to_bottom' the top half onto the bottom "
        "(a reflection in water, about a horizontal line across the middle), 'bottom_to_top' the bottom half onto "
        "the top. For a picture symmetric in four quarters, name the quarter that is copied into the other three: "
        "'top_left', 'top_right', 'bottom_left' or 'bottom_right'. On a picture with an odd width or height the "
        "middle column or row lies on the line and stays.",
    )


@effect(category="distortion", params=MirrorParams)
def mirror(rgb: np.ndarray, params: MirrorParams) -> np.ndarray:
    """Make the picture symmetric: keep one half (or quarter) and replace the rest by its reflection.

    `direction` names the copy: 'left_to_right' keeps the left half and reflects it onto the right, and so on for
    the other halves; 'top_left' and the other three corners keep that quarter and reflect it across both lines.
    The kept part is copied untouched, pixel for pixel, so the result is exactly symmetric and uses only the
    picture's own values.
    """
    out = rgb
    for dimension, kept in enumerate(_KEPT[params.direction]):  # rows, then columns
        if kept is None:
            continue
        size = rgb.shape[dimension]
        position = np.arange(size)
        # each position reads the kept side: the smaller of itself and its reflection keeps the first half, the
        # larger the last
        reflection = size - 1 - position
        source = np.minimum(position, reflection) if kept == "first" else np.maximum(position, reflection)
        out = np.take(out, source, axis=dimension)
    return out.copy() if out is rgb else out
