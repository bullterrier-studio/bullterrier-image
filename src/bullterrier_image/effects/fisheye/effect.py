"""The `fisheye` effect: the picture seen through a wide-angle fisheye lens, the middle magnified."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class FisheyeParams(EffectParams):
    """A fisheye: the picture bent as an equidistant fisheye lens bends a flat scene, so the middle is magnified and
    the periphery squeezed."""

    amount: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=0.9,
        doc="How wide the lens sees, as a fraction of 90 degrees: the half angle of its field of view, reaching the "
        "farthest corner of the picture, is amount x 90 degrees (0.5 is a 90 degree lens, 0.9 a 162 degree one). "
        "Larger bends the picture more: the middle swells and the edges are squeezed in toward the corner; 0 "
        "leaves the picture unchanged. Wider than 0.9 is not offered, as the middle would grow beyond all reason.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The lens axis's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The lens axis's vertical position as a fraction of the way down the picture: 0 = the first pixel row, "
        "0.5 = the middle, 1 = the last.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but the magnified middle turns blocky), 'bilinear' blends the four pixels "
        "around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, "
        "a little slower, and may ring beside hard edges).",
        advanced=True,
    )


@effect(category="distortion", params=FisheyeParams)
def fisheye(rgb: np.ndarray, params: FisheyeParams) -> np.ndarray:
    """Bend the picture as a fisheye lens with its axis at `x`, `y` would, magnifying the middle.

    The picture is taken as a flat scene seen by a lens that sees a half angle of `amount` x 90 degrees out to the
    farthest corner, and drawn as an equidistant fisheye (distance from the axis proportional to the angle). A pixel at
    distance d from the axis, the farthest corner at R and w the half angle, shows the picture at R x tan(d / R x w) /
    tan(w) on the same ray: always nearer the axis than the pixel, and exactly there at the axis and at the corner. So
    the whole result lies inside the picture, with no edge to fill, and never leaves 0..1.
    """
    height, width = rgb.shape[:2]
    centre = Centre(height, width, x=params.x, y=params.y)
    # the farthest corner, in pixels: the lens's field of view ends there
    farthest = np.hypot(max(params.y, 1 - params.y) * (height - 1), max(params.x, 1 - params.x) * (width - 1))
    if params.amount == 0 or farthest == 0:
        return rgb.copy()
    half_angle = params.amount * np.pi / 2  # 90 degrees is a right angle: pi / 2 radians
    distance = centre.distance
    shown_from = farthest * np.tan(distance / farthest * half_angle) / np.tan(half_angle)
    # the ratio of the two distances; at the axis itself, the limit w / tan(w) of tan(d w / R) / (d tan w / R)
    scale = np.divide(
        shown_from, distance, out=np.full_like(distance, half_angle / np.tan(half_angle)), where=distance > 0
    )
    rows, cols = centre.sources(scale=scale)
    # a pixel never reads from beyond where it is shown, so none reads beyond the picture, whatever the edge rule
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge="nearest")
