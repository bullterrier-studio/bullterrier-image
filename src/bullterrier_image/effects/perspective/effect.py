"""The `perspective` effect: the picture tilted as a camera sees it: keystoned, skewed, turned and zoomed in place."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class PerspectiveParams(EffectParams):
    """A perspective correction: a keystone and a skew along each axis, then a turn and a zoom about the centre."""

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
    shear_x: float = param(
        0.0,
        unit="ratio",
        min=-2,
        max=2,
        doc="The horizontal skew, after the keystones, as a slope in columns per row: each row slides sideways by this "
        "many pixels for every pixel it lies below the centre (and the other way above it), so a vertical line leans "
        "into a line of this slope. 0 = none, 0.2 = a lean of about 11 degrees, 1 = 45 degrees, positive slides the "
        "lower half right and the upper half left, negative the other way. Applied before `shear_y`.",
    )
    shear_y: float = param(
        0.0,
        unit="ratio",
        min=-2,
        max=2,
        doc="The vertical skew, after `shear_x`, as a slope in rows per column: each column slides down by this many "
        "pixels for every pixel it lies right of the centre (and up on the left of it), so a horizontal line tilts "
        "into a line of this slope. 0 = none, positive tilts the right side down, negative the right side up.",
    )
    rotate: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        doc="How far the picture is turned about its centre, after the keystones and skews, in degrees: positive turns "
        "clockwise, negative counter-clockwise; 0 = none. The corners it uncovers are filled by `edge`.",
    )
    scale: float = param(
        0.0,
        unit="fraction",
        min=-0.9,
        max=4,
        doc="How much the picture is enlarged about its centre, after the keystones, skews and turn, as a fraction of "
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
        doc="What fills the margins the keystone, skew, turn and zoom uncover around the picture: 'reflect' (mirrored, "
        "edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated: "
        "streaks), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=PerspectiveParams)
def perspective(rgb: np.ndarray, params: PerspectiveParams) -> np.ndarray:
    """Tilt the picture as a camera would see it: keystone it by `vertical` then `horizontal`, skew, turn and zoom it.

    The picture is moved by one projective transform about its centre, which keeps straight lines straight: first the
    vertical keystone (the far edge shrinks to 1 - `vertical` of the near one, the near edge keeps its width), then the
    horizontal one, then the skew (rows slide sideways by `shear_x` px per row, then columns down by `shear_y` px per
    column, so the area never changes), then `rotate` degrees clockwise, then `scale`. With all six at 0 the picture is
    returned unchanged. The output beyond the vanishing line reads from outside the picture, so `edge` says what it
    shows.
    """
    height, width = rgb.shape[:2]
    half_height, half_width = height / 2, width / 2  # the picture's edges are at the pixels' edges
    # A keystone is [[a, 0, 0], [0, a, 0], [c, 0, 1]]-like: a point's depth grows linearly across the picture, by the
    # fraction `slope` of the far side's, and `a` keeps the near edge's width.
    slope_v = -params.vertical / (2 - abs(params.vertical))
    slope_h = params.horizontal / (2 - abs(params.horizontal))
    keystone_v = np.array([[1 - abs(slope_v), 0, 0], [0, 1 - abs(slope_v), 0], [0, slope_v / half_height, 1]])
    keystone_h = np.array([[1 - abs(slope_h), 0, 0], [0, 1 - abs(slope_h), 0], [slope_h / half_width, 0, 1]])
    skew_x = np.array([[1, params.shear_x, 0], [0, 1, 0], [0, 0, 1]])
    skew_y = np.array([[1, 0, 0], [params.shear_y, 1, 0], [0, 0, 1]])
    angle = np.radians(params.rotate)
    turn = np.array([[np.cos(angle), -np.sin(angle), 0], [np.sin(angle), np.cos(angle), 0], [0, 0, 1]])
    zoom = np.array([[1 + params.scale, 0, 0], [0, 1 + params.scale, 0], [0, 0, 1]])
    # from a point of the output to the point it shows
    back = np.linalg.inv(zoom @ turn @ skew_y @ skew_x @ keystone_h @ keystone_v)
    dr, dc = Centre.middle(height, width).offsets()
    depth = back[2, 0] * dc + back[2, 1] * dr + back[2, 2]
    rows = np.arange(height)[:, None] - dr + (back[1, 0] * dc + back[1, 1] * dr + back[1, 2]) / depth
    cols = np.arange(width)[None, :] - dc + (back[0, 0] * dc + back[0, 1] * dr + back[0, 2]) / depth
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
