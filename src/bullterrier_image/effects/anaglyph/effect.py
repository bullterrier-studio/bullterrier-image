"""The `anaglyph` effect: a red and a cyan view of the picture's brightness, moved apart sideways, as in 3D glasses."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.shift import shift


class AnaglyphParams(EffectParams):
    """A red-cyan anaglyph: the brightness seen by the left eye in red and by the right eye in cyan (green and blue),
    each moved `shift` px sideways from where it was."""

    shift: float = param(
        6.0,
        unit="px",
        min=0,
        max=200,
        doc="How far each eye's view is moved from the picture, in px tuned for a picture about 480 px high: the red "
        "(left) view moves this far to the left and the cyan (right) view this far to the right, so they end up twice "
        "this far apart and objects seem to float out of the screen through red-cyan glasses. 0 gives a grey "
        "picture, the brightness in all three channels. A fraction of a pixel blends the two pixels it lies between.",
    )
    edge: Literal["nearest", "wrap", "reflect", "mirror"] = param(
        "nearest",
        doc="What a moved view shows where it has left the picture's edge: 'nearest' repeats the view's edge pixels "
        "(a coloured smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the "
        "view (the edge line repeated once), 'mirror' mirrors it about the edge line.",
        advanced=True,
    )


@effect(category="glitch", params=AnaglyphParams)
def anaglyph(rgb: np.ndarray, params: AnaglyphParams) -> np.ndarray:
    """Red from the brightness moved `shift` px left, green and blue from the brightness moved `shift` px right.

    The brightness is Rec. 709 luminance. The picture is grey (red = green = blue = luminance) with a red fringe on
    one side of every edge and a cyan one on the other, which red-cyan glasses show at different depths. A shift
    of 0 leaves only the grey picture; what a view shows beyond the picture's edge is set by `edge`.
    """
    brightness = luminance(rgb)
    left = shift(brightness, 0, -params.shift, edge=params.edge)  # constant: the views differ sideways only
    right = shift(brightness, 0, params.shift, edge=params.edge)  # constant: no vertical movement
    return np.stack([left, right, right], axis=-1)
