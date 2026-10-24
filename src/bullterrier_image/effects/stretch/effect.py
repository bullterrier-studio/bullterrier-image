"""The `stretch` effect: one row or column of the picture smeared outward over its neighbours."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class StretchParams(EffectParams):
    """A pixel stretch: one line of the picture repeated over the lines beside it, the way a glitched file smears."""

    axis: Literal["vertical", "horizontal"] = param(
        "vertical",
        doc="Which way the line is smeared: 'vertical' repeats one pixel row up and down over the rows beside it "
        "(streaks run down the picture), 'horizontal' repeats one pixel column left and right (streaks run across).",
    )
    at: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the line that is smeared lies, as a fraction 0 to 1 of the way across the picture (down it for "
        "'vertical', right for 'horizontal'): 0 = the first row or column, 0.5 = the middle, 1 = the last. Moving it "
        "chooses which colours the smear carries.",
    )
    span: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the line is smeared, as a fraction 0 to 1 of the picture's height (width for 'horizontal') on "
        "each side that `side` names, rounded to whole pixels. 0 = nothing is smeared; 1 = the line covers the whole "
        "picture. Raising it hides more of the picture behind the smear.",
    )
    side: Literal["both", "after", "before"] = param(
        "both",
        doc="Which sides of the line are smeared: 'both' (outward each way), 'after' (down, or right) or 'before' "
        "(up, or left). The other side keeps the picture as it is.",
    )


@effect(category="distortion", params=StretchParams)
def stretch(rgb: np.ndarray, params: StretchParams) -> np.ndarray:
    """Repeat one row (or column) of the picture over the `span` of rows (columns) beside it.

    The line sits at `at` and keeps its place; the lines within `span` of it on the chosen `side` become copies of it,
    the rest of the picture is untouched. Every output pixel is an input pixel, so no colour is invented.
    """
    axis = int(params.axis == "horizontal")  # array axis 0 runs down the rows, 1 across the columns
    size = rgb.shape[axis]
    line = round(params.at * (size - 1))
    reach = round(params.span * size)
    place = np.arange(size)
    offset = place - line  # negative before the line, positive after it
    smeared = np.zeros(size, dtype=bool)
    if params.side != "after":
        smeared |= (offset < 0) & (offset >= -reach)
    if params.side != "before":
        smeared |= (offset > 0) & (offset <= reach)
    return np.take(rgb, np.where(smeared, line, place), axis=axis)
