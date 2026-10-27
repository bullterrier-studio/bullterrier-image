"""The `lens_distortion` effect: barrel or pincushion distortion, or a fisheye, with the picture scaled back."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import Centre, remap


class LensDistortionParams(EffectParams):
    """Lens distortion: the picture bent about the lens axis, by the radial polynomial of the Brown-Conrady model or
    as through a fisheye lens, so straight lines bow outward (barrel) or inward (pincushion)."""

    model: Literal["brown_conrady", "fisheye"] = param(
        "brown_conrady",
        doc="The curve of the bend, with the same `amount` giving the same stretch at the farthest corner: "
        "'brown_conrady' is the radial polynomial of the Brown-Conrady lens model (k1 only), which bends evenly "
        "from the axis out, as an ordinary lens does; 'fisheye' is an equidistant fisheye, a pixel's distance from "
        "the axis proportional to the angle it sees, which keeps the middle almost flat and bends the edges "
        "hardest, and for a negative `amount` is its inverse, an arctangent curve.",
    )
    amount: float = param(
        0.2,
        unit="ratio",
        min=-0.3,
        max=3.5,
        doc="How strongly the lens bends: at the farthest corner each pixel's source lies 1 + amount times as far "
        "from the axis as the pixel's own place, where at the axis it lies at its own place (0.2 is 20% farther; "
        "'fit' then scales it back). The distance from the axis is measured in units of the farthest corner's. "
        "Positive is barrel: straight lines bow outward, the middle swells and the picture looks wrapped around a "
        "ball. Negative is pincushion: lines bow inward and the corners are pulled out; -0.3 is the most the "
        "'brown_conrady' curve can have without folding the picture over. 0 leaves the picture unchanged. For a "
        "fisheye, 0.4 is about a 103 degree lens, 1 a 134 degree one and 3.5 a 162 degree one.",
    )
    fit: Literal["frame", "centre"] = param(
        "frame",
        doc="How the picture is scaled back after the bend: 'frame' keeps the farthest corner in its corner, so the "
        "picture fills the frame (a barrel is zoomed in and loses its sides, and nothing beyond the picture is "
        "shown; a pincushion is zoomed out toward the middle and pulls in what lies beyond the middle of each edge, "
        "which the edge rule fills); 'centre' keeps the scale at the lens axis, so a "
        "barrel shrinks toward the middle, and the edge rule fills the corners it exposes.",
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
    border: Color | None = param(
        None,
        doc="The colour shown where the bend reads from beyond the picture (with fit 'centre', or a pincushion): "
        "'#rrggbb', '#rgb' or [r, g, b] 0..1; none (the default) fills it by the `edge` rule instead. A source "
        "less than a pixel beyond the picture is blended into the border, so its edge is smooth. A black border "
        "is a bezel round a curved screen.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but lines turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What shows where the bend reads from beyond the picture (with fit 'centre', or a pincushion): "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge). With a `border`, only what the bend reads from within a pixel "
        "of the edge, which blends into the border.",
        advanced=True,
    )


def _fisheye_reach(amount: float) -> float:
    """The `u` for which the fisheye curve stretches the farthest corner by `amount`: tan(u) / u = 1 + amount for a
    positive amount, atan(u) / u = 1 + amount for a negative one. Tangent rises and arctangent falls steadily in
    u, so halving a bracket finds it."""
    law = np.tan if amount > 0 else np.arctan
    low, high = 0.0, (np.pi / 2 if amount > 0 else 2 / (1 + amount))  # constant: tan reaches infinity at pi / 2
    for _ in range(60):  # constant: 60 halvings shrink a bracket of a few radians below double precision
        middle = (low + high) / 2
        stretch = law(middle) / middle  # rises with u for tan, falls for atan
        if (stretch < 1 + amount) == (amount > 0):
            low = middle
        else:
            high = middle
    return (low + high) / 2


@effect(category="distortion", params=LensDistortionParams)
def lens_distortion(rgb: np.ndarray, params: LensDistortionParams) -> np.ndarray:
    """Bend the picture as a lens with the curve `model` and strength `amount` about the axis `x`, `y` does.

    Each pixel at distance d from the axis shows the picture on the same ray at d x s(d / R), R the distance of the
    farthest corner. s is 1 at the axis and 1 + `amount` at the farthest corner: for 'brown_conrady' 1 + `amount` x
    (d / R)^2 (the Brown-Conrady radial term, k1 only); for 'fisheye' tan(u d / R) / (u d / R), the equidistant
    fisheye for a half field of view u (the arctangent, its inverse, for a negative `amount`). With `fit` 'frame' s is
    divided by 1 + `amount` so that the corner stays put (a barrel then reads only inside the picture). The axis never
    moves, and the result never leaves 0..1. What the bend reads from beyond the picture is the `edge` rule's, or
    `border` where that is set.
    """
    height, width = rgb.shape[:2]
    # the farthest corner, in pixels: where the radius is 1
    farthest = np.hypot(max(params.y, 1 - params.y) * (height - 1), max(params.x, 1 - params.x) * (width - 1))
    if params.amount == 0 or farthest == 0:
        return rgb.copy()
    centre = Centre(height, width, x=params.x, y=params.y)
    radius = centre.distance / farthest
    if params.model == "brown_conrady":
        scale = 1 + params.amount * radius**2
    else:
        law = np.tan if params.amount > 0 else np.arctan
        angle = _fisheye_reach(params.amount) * radius
        # at the axis itself, the limit 1 of law(angle) / angle
        scale = np.divide(law(angle), angle, out=np.ones_like(angle), where=angle > 0)
    if params.fit == "frame":
        scale = scale / (1 + params.amount)
    rows, cols = centre.sources(scale=scale)
    out = remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
    if params.border is None:
        return out
    # reading from less than a pixel beyond the first or last pixel blends it into the border; farther out is all border
    inside_rows = np.clip(np.minimum(rows + 1, height - rows), 0, 1)
    inside_cols = np.clip(np.minimum(cols + 1, width - cols), 0, 1)
    picture = (inside_rows * inside_cols).astype(np.float32)[..., None]
    return (out * picture + np.asarray(params.border, np.float32) * (1 - picture)).astype(np.float32)
