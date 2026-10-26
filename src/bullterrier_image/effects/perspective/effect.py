"""The `perspective` effect: the picture tilted as a camera sees it, keystoned, then turned and zoomed in place."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class PerspectiveParams(EffectParams):
    """A perspective correction: a keystone along each axis, then a turn and a zoom about the picture's centre."""

    vertical: float = param(
        0.25,
        unit="fraction",
        min=-0.9,
        max=0.9,
        doc="The vertical keystone: the fraction by which the far edge shrinks while the near one keeps its width, "
        "straight lines staying straight, as when a camera tilts back to shoot a tall building. Positive makes the "
        "top the far side (the top edge narrows to 1 - vertical of the bottom edge), negative makes the bottom the "
        "far side; 0 = none. Raising it makes a steeper converging; it stops short of 1, where the far edge would "
        "shrink to a point.",
    )
    horizontal: float = param(
        0.0,
        unit="fraction",
        min=-0.9,
        max=0.9,
        doc="The horizontal keystone: the fraction by which the far edge shrinks while the near one keeps its "
        "height, as when a camera turns to the side. Positive makes the right side the far side (the right edge "
        "shortens to 1 - horizontal of the left edge), negative makes the left side the far side; 0 = none. Applied "
        "after the vertical keystone.",
    )
    rotate: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        doc="How far the picture is turned about its centre, after the keystones, in degrees: positive turns "
        "clockwise, negative counter-clockwise; 0 = none. The corners it uncovers are filled by `edge`.",
    )
    scale: float = param(
        0.0,
        unit="fraction",
        min=-0.9,
        max=4,
        doc="How much the picture is enlarged about its centre, after the keystones and the turn, as a fraction of "
        "its size: 0 = unchanged, 0.5 = half as big again, -0.5 = half the size (the empty margin is filled by "
        "`edge`). Enlarging it hides the corners that the keystone and turn uncover.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What fills the margins the keystone, turn and zoom uncover around the picture: 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated: streaks), "
        "'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=PerspectiveParams)
def perspective(rgb: np.ndarray, params: PerspectiveParams) -> np.ndarray:
    """Tilt the picture as a camera would see it: keystone it by `vertical` then `horizontal`, turn it, zoom it.

    The picture is moved by one projective transform about its centre, which keeps straight lines straight: first the
    vertical keystone (the far edge shrinks to 1 - `vertical` of the near one, the near edge keeps its width), then the
    horizontal one, then `rotate` degrees clockwise, then `scale`. With all four at 0 the picture is returned
    unchanged. The output beyond the vanishing line reads from outside the picture, so `edge` says what it shows.
    """
    height, width = rgb.shape[:2]
    half_height, half_width = height / 2, width / 2  # the picture's edges are at the pixels' edges
    # A keystone is [[a, 0, 0], [0, a, 0], [c, 0, 1]]-like: a point's depth grows linearly across the picture, by the
    # fraction `slope` of the far side's, and `a` keeps the near edge's width.
    slope_v = -params.vertical / (2 - abs(params.vertical))
    slope_h = params.horizontal / (2 - abs(params.horizontal))
    keystone_v = np.array([[1 - abs(slope_v), 0, 0], [0, 1 - abs(slope_v), 0], [0, slope_v / half_height, 1]])
    keystone_h = np.array([[1 - abs(slope_h), 0, 0], [0, 1 - abs(slope_h), 0], [slope_h / half_width, 0, 1]])
    angle = np.radians(params.rotate)
    turn = np.array([[np.cos(angle), -np.sin(angle), 0], [np.sin(angle), np.cos(angle), 0], [0, 0, 1]])
    zoom = np.array([[1 + params.scale, 0, 0], [0, 1 + params.scale, 0], [0, 0, 1]])
    back = np.linalg.inv(zoom @ turn @ keystone_h @ keystone_v)  # from a point of the output to the point it shows
    dr, dc = Centre.middle(height, width).offsets()
    depth = back[2, 0] * dc + back[2, 1] * dr + back[2, 2]
    rows = np.arange(height)[:, None] - dr + (back[1, 0] * dc + back[1, 1] * dr + back[1, 2]) / depth
    cols = np.arange(width)[None, :] - dc + (back[0, 0] * dc + back[0, 1] * dr + back[0, 2]) / depth
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
