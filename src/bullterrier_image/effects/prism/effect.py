"""The `prism` effect: the picture split into a spectrum of copies, each scaled a little differently about a centre."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap

# The spectrum as a simple rule, not a colour-matching table: the pure colours of the HSV colour wheel from red to blue
# (red, yellow, green, cyan, blue), equally spaced, with the weight of each channel changing linearly between them.
# constant: the five pure colours of the standard HSV colour wheel from red to blue, one RGB row each
_SPECTRUM = np.array([[1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 1, 1], [0, 0, 1]], np.float64)


class PrismParams(EffectParams):
    """A prism: the picture is read at `samples` slightly different scales about a centre, one for each colour of the
    spectrum from red to blue, and the copies are added back up by the colour they stand for."""

    amount: float = param(
        18.0,
        unit="px",
        min=0,
        max=100,
        doc="How far the reddest and the bluest copy are pulled apart, in px at the farthest corner of the picture, "
        "tuned for a picture about 480 px high: there the red copy is shifted half this far outward from the centre "
        "and the blue copy half this far inward, the copies between in steps. The shift grows in proportion to the "
        "distance from the centre, so it is half as much halfway out and nothing at the centre. A larger amount is a "
        "wider rainbow fringe on every edge; 0 leaves the picture unchanged. A shift larger than the distance to the "
        "farthest corner is limited to it.",
    )
    samples: int = param(
        7,
        unit="count",
        min=3,
        max=32,
        doc="How many copies make the spectrum, from red to blue in equal steps of scale: 3 gives a red, a green and a "
        "blue copy, as a lens's colour fringe does; more copies fill in yellow and cyan between them and turn the "
        "fringe into a smooth rainbow, but cost more time. Fewer copies than the amount needs show as separate "
        "ghosts.",
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
        doc="How a point between pixels is read from a copy: 'nearest' takes the closest pixel (fastest, but the "
        "fringes step), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic "
        "spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the copies see beyond the picture's edge (the bluer ones read from farther out than they show): "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="glitch", params=PrismParams)
def prism(rgb: np.ndarray, params: PrismParams) -> np.ndarray:
    """Add up `samples` copies of the picture, scaled about the centre `x`, `y` and coloured as a spectrum.

    Copy k stands for the colour k / (samples - 1) of the way from red to blue and shows each pixel d px from the centre
    read at d x (1 - e x (1 - 2k / (samples - 1))), with e = `amount` / 2 / R, R the distance of the farthest corner
    (red is shifted outward, blue inward, the middle copy stays). A channel of the result is the weighted average of
    that channel in the copies, the weights following the simple spectrum above (red from the red and yellow copies,
    green from yellow to cyan, blue from cyan and blue) and adding up to 1, so a flat picture and the centre are
    unchanged and the result never leaves 0..1.
    """
    height, width = rgb.shape[:2]
    # the farthest corner, in pixels: where the shift is the whole amount
    farthest = np.hypot(max(params.y, 1 - params.y) * (height - 1), max(params.x, 1 - params.x) * (width - 1))
    if farthest == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    shift = min(params.amount / 2 / farthest, 1.0)  # as a fraction of the distance from the centre, up to all of it
    along = np.arange(params.samples) / (params.samples - 1)  # 0 for red to 1 for blue
    colours = np.arange(len(_SPECTRUM))  # the spectrum's colours are one step apart
    weights = np.stack(
        [np.interp(along * colours[-1], colours, _SPECTRUM[:, channel]) for channel in range(rgb.shape[-1])], axis=-1
    )
    weights /= weights.sum(axis=0)  # each channel is an average, so a flat picture stays as it is
    out = np.zeros_like(rgb)
    for copy, weight in enumerate(weights):
        rows, cols = centre.sources(scale=1 - shift * (1 - 2 * along[copy]))
        for channel in np.flatnonzero(weight):
            seen = remap(
                rgb[..., channel : channel + 1], rows, cols, interpolation=params.interpolation, edge=params.edge
            )
            out[..., channel] += np.float32(weight[channel]) * seen[..., 0]
    return np.clip(out, 0.0, 1.0)
