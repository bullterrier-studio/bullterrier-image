"""The `kaleidoscope` effect: the picture folded about a centre into repeated sectors, each a wedge and its mirror."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class KaleidoscopeParams(EffectParams):
    """A kaleidoscope: a wedge of the picture and its mirror image repeated round a centre."""

    segments: int = param(
        6,
        unit="count",
        min=2,
        max=64,
        doc="How many times the pattern repeats round the centre: the full circle is cut into this many equal sectors, "
        "each holding a wedge of the picture (half a sector wide) and its mirror image, so every wedge borders its own "
        "reflection and the seams between them do not show. 2 = a picture mirrored across a line through the centre "
        "and again across the line at right angles; raising it makes more, narrower wedges.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="Where the pattern starts, as the direction in degrees, clockwise from pointing right, of the first mirror "
        "axis and of the wedge that follows it: 0 puts an axis along the line right of the centre; turning it turns "
        "the mirror axes, and shows a different wedge of the picture. The pattern repeats every 360 / segments "
        "degrees.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = "
        "the middle, 1 = the last.",
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
        doc="What the kaleidoscope sees beyond the picture's edge (a wedge reaches past it in the corners, and "
        "wherever the centre is off to one side): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about "
        "the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=KaleidoscopeParams)
def kaleidoscope(rgb: np.ndarray, params: KaleidoscopeParams) -> np.ndarray:
    """Fold the picture about the centre `x`, `y` into `segments` sectors, each a wedge of the picture and its mirror.

    A pixel at angle a about the centre (clockwise from pointing right) and distance d shows the picture at the same
    distance d and the angle that a folds to: a less `angle`, taken modulo the sector 360 / `segments`, and if that
    passes half a sector, mirrored to what it falls short of the sector by, then `angle` added back. The wedge from
    `angle` for half a sector is the picture as it is, the next half a sector its mirror image, and so on round the
    centre, which keeps its own pixel.
    """
    height, width = rgb.shape[:2]
    centre = Centre(height, width, x=params.x, y=params.y)
    dr, dc = centre.offsets()
    sector = 2 * np.pi / params.segments
    start = np.radians(params.angle)
    pixel_angle = np.arctan2(dr, dc)  # clockwise from pointing right, as rows run down
    into_sector = (pixel_angle - start) % sector
    shown_angle = start + np.minimum(into_sector, sector - into_sector)  # the sector's second half runs back
    rows, cols = centre.sources(turn=pixel_angle - shown_angle)  # `turn` is positive counter-clockwise on the picture
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
