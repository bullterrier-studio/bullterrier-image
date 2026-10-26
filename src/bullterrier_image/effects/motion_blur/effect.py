"""The `motion_blur` effect: the picture smeared along a straight line, as by a moving camera or subject."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.convolve import convolve


class MotionBlurParams(EffectParams):
    """A motion blur: every pixel becomes the average of the picture along a straight line through it."""

    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction of the motion, counter-clockwise as seen in the picture: 0 = horizontal, 90 = vertical, "
        "45 = rising to the right. Opposite directions (0 and 180) give the same blur.",
    )
    length: float = param(
        24.0,
        unit="px",
        min=0,
        max=500,
        drivable=True,
        doc="How far the smear reaches along the line, end to end, in px at the recipe's size: larger is a faster "
        "motion; 0 leaves the picture unchanged.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


def _line_kernel(angle: float, length: float) -> np.ndarray:
    """The weights of a straight line `length` px long through the middle of a (2r + 1) x (2r + 1) square, r = ceil(
    length / 2): each pixel weighs the length of line inside it (a 1 x 1 square), the weights summing to 1. Rows count
    downwards, columns to the right; `angle` is counter-clockwise from the columns' direction."""
    reach = int(np.ceil(length / 2))
    rows, cols = np.mgrid[-reach : reach + 1, -reach : reach + 1].astype(np.float64)
    direction = (np.cos(np.radians(angle)), -np.sin(np.radians(angle)))  # (right, down)
    # the stretch of the line, as distance t from its middle, inside each pixel: where it is within half a pixel of the
    # pixel's centre along both axes; both limits of that stretch are found per axis and intersected
    low, high = np.full(rows.shape, -length / 2), np.full(rows.shape, length / 2)
    for centre, step in ((cols, direction[0]), (rows, direction[1])):
        if step == 0:  # the line never leaves this row or column: it is inside the pixel's span or never in it
            inside = np.abs(centre) <= 0.5  # constant: half a pixel: the distance from a pixel's centre to its edge
            low, high = np.where(inside, low, np.inf), np.where(inside, high, -np.inf)
        else:
            ends = np.stack([(centre - 0.5) / step, (centre + 0.5) / step])
            low, high = np.maximum(low, ends.min(axis=0)), np.minimum(high, ends.max(axis=0))
    weights = np.maximum(high - low, 0.0)
    return weights / weights.sum()


@effect(category="light", params=MotionBlurParams)
def motion_blur(rgb: np.ndarray, params: MotionBlurParams) -> np.ndarray:
    """Smear the picture along a straight line of `length` px at `angle` degrees, as a moving camera would.

    Every pixel becomes the average of the picture along that line, each colour channel on its own, so flat areas stay
    flat, the mean is kept and the result never leaves the picture's own range. The cost hardly grows with the length.
    """
    if params.length == 0:
        return rgb.copy()
    return convolve(rgb, _line_kernel(params.angle, params.length), edge=params.edge)
