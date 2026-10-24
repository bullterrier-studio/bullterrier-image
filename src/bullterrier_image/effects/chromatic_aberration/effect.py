"""The `chromatic_aberration` effect: a lens's colour fringes, red and blue magnified differently from green."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class ChromaticAberrationParams(EffectParams):
    """Lateral chromatic aberration: the colour channels magnified a little differently about a centre, so edges
    away from it grow a red fringe on one side and a blue one on the other."""

    amount: float = param(
        4.0,
        unit="px",
        min=0,
        max=100,
        doc="How far red and blue are pulled apart, in pixels at the farthest corner of the picture: there the red "
        "is shifted this far outward from the centre and the blue this far inward, while green stays where it is. "
        "The shift grows in proportion to the distance from the centre, so it is half as much halfway out and "
        "nothing at the centre. A larger amount is wider fringes; 0 leaves the picture unchanged. A shift larger "
        "than the distance to the farthest corner is limited to it.",
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
        doc="How a point between pixels is read from a colour channel: 'nearest' takes the closest pixel (fastest, "
        "but the fringes step), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a "
        "cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blue sees beyond the picture's edge (it reads from farther out than it shows): 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=ChromaticAberrationParams)
def chromatic_aberration(rgb: np.ndarray, params: ChromaticAberrationParams) -> np.ndarray:
    """Shift red outward and blue inward from the centre `x`, `y` by up to `amount` px, leaving green alone.

    Each pixel d px from the centre shows red read at d x (1 - amount / R) and blue at d x (1 + amount / R) on the
    same ray, R the distance of the farthest corner, so the shift grows linearly from nothing at the centre to
    `amount` px there; green is the picture itself. Each channel is read by `interpolation`; the result never leaves
    0..1.
    """
    height, width = rgb.shape[:2]
    # the farthest corner, in pixels: where the shift is the whole amount
    farthest = np.hypot(max(params.y, 1 - params.y) * (height - 1), max(params.x, 1 - params.x) * (width - 1))
    if params.amount == 0 or farthest == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    shift = min(params.amount / farthest, 1.0)  # as a fraction of the distance from the centre, up to all of it

    def shifted(channel: np.ndarray, scale: float) -> np.ndarray:
        """One colour channel (height, width, 1) read at `scale` times each pixel's distance from the centre."""
        rows, cols = centre.sources(scale=scale)
        return remap(channel, rows, cols, interpolation=params.interpolation, edge=params.edge)[..., 0]

    out = rgb.copy()
    out[..., 0] = shifted(rgb[..., :1], 1 - shift)  # red, magnified: shifted outward
    out[..., 2] = shifted(rgb[..., 2:], 1 + shift)  # blue, shrunk: shifted inward
    return out
