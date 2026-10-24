"""The `polar` effect: the picture wrapped round a centre like a tiny planet, or a planet unwrapped into a picture."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class PolarParams(EffectParams):
    """A polar warp: rectangular coordinates to polar (the picture wrapped round a centre) or back."""

    mode: Literal["to", "from"] = param(
        "to",
        doc="Which way to convert: 'to' wraps the picture round the centre into a tiny planet (its bottom row at the "
        "centre, its top row on the rim of a circle as wide as the shorter side, the left edge straight up and the "
        "columns running clockwise, so the sky of a panorama ends up outside); 'from' is the opposite, it takes a "
        "picture that is such a planet round the centre and unwraps it into a flat picture, so 'from' undoes 'to'.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The planet's centre horizontal position as a fraction of the way across the picture: 0 = the first "
        "pixel column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The planet's centre vertical position as a fraction of the way down the picture: 0 = the first pixel "
        "row, 0.5 = the middle, 1 = the last.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but the middle of a planet turns blocky), 'bilinear' blends the four pixels "
        "around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, "
        "a little slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What lies beyond the planet's rim in 'to', or beyond the picture where an off-centre planet leaves it in "
        "'from' (the angle itself never has an edge; it wraps round): 'nearest' (the edge pixel repeated: the sky "
        "stays one flat tone of the top row), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the "
        "edge pixel), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=PolarParams)
def polar(rgb: np.ndarray, params: PolarParams) -> np.ndarray:
    """Wrap the picture round the centre `x`, `y` into a planet (`mode` 'to'), or unwrap such a planet ('from').

    The planet has the radius of half the shorter side, from the first pixel to the last. 'to': a pixel d px from the
    centre shows the row (1 - d / radius) down the picture, from the top, and the column that is the fraction of a
    whole turn, clockwise from straight up, of the width; the columns wrap round, so the seam shows nothing. 'from':
    each pixel reads the planet at the distance and angle that its row and column name, the same relation the other way.
    """
    height, width = rgb.shape[:2]
    reach = (min(height, width) - 1) / 2  # the planet's radius in pixels
    if reach == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    if params.mode == "to":
        dr, dc = centre.offsets()
        turn = np.arctan2(
            dc, -dr
        )  # clockwise from straight up, in radians; the columns wrap round, so a negative one is fine
        rows = (1 - centre.distance / reach) * (height - 1)
        cols = turn / (2 * np.pi) * width
    else:
        down = np.arange(height, dtype=np.float64)[:, None] / (height - 1)
        out_of = (1 - down) * reach  # distance from the centre, 0 at the bottom row and the reach at the top
        turn = np.arange(width, dtype=np.float64)[None, :] / width * 2 * np.pi
        rows = params.y * (height - 1) - out_of * np.cos(turn)
        cols = params.x * (width - 1) + out_of * np.sin(turn)
    return remap(
        rgb, rows, cols, interpolation=params.interpolation, edge=params.edge, wrap_columns=params.mode == "to"
    )
