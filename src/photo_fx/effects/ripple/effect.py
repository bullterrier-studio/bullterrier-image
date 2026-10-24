"""The `ripple` effect: a wave travelling across the picture that moves each pixel sideways to its own direction."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.warp import Centre, remap


class RippleParams(EffectParams):
    """A ripple: a sine wave of displacement, moving each pixel perpendicular to the direction the wave travels."""

    wavelength: float = param(
        40.0,
        unit="px",
        min=2,
        max=4000,
        doc="The distance in pixels from one crest of the wave to the next, along the wave's direction. Raising it "
        "gives long, lazy swells; lowering it gives tight, fine ripples (below about 4 px the wave is too fine for "
        "the pixels to show it).",
    )
    amplitude: float = param(
        6.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels the wave moves a pixel at its crest, perpendicular to the wave's direction. Raising it "
        "makes taller waves and a more broken-up picture; 0 leaves the picture unchanged.",
    )
    phase: float = param(
        0.0,
        unit="degrees",
        min=-360,
        max=360,
        doc="Where the wave starts, in degrees of one cycle: 0 has the wave at zero along the line through the "
        "picture's centre and rising away from it in the wave's direction, 90 has a crest there, and 360 is a whole "
        "cycle and so the same as 0. Animating it makes the wave travel.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="The direction the wave travels in, in degrees clockwise from pointing right: 0 sends the wave along the "
        "picture's width, so each column is moved up or down and the crests are horizontal ripples running across; 90 "
        "sends it down the picture, so each row is moved left or right. Pixels are always moved at a right angle to "
        "this direction.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the wave sees beyond the picture's edge (it moves pixels in from outside along the edges): "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
    )


@effect(category="distortion", params=RippleParams)
def ripple(rgb: np.ndarray, params: RippleParams) -> np.ndarray:
    """Move every pixel by a sine wave of `wavelength` and `amplitude` that travels in the direction `angle`.

    A pixel at distance s px from the picture's centre measured along the wave's direction (`angle` degrees clockwise
    from pointing right) is moved by amplitude x sin(2 pi s / wavelength + phase) px, at a right angle to that
    direction (down for angle 0, left for angle 90), and the point between pixels is read by `interpolation`. The wave
    is at its phase along the line through the centre, so turning `angle` turns the whole pattern about the centre.
    """
    height, width = rgb.shape[:2]
    angle = np.radians(params.angle)
    dr, dc = Centre.middle(height, width).offsets()
    along = dc * np.cos(angle) + dr * np.sin(angle)
    moved = params.amplitude * np.sin(2 * np.pi * along / params.wavelength + np.radians(params.phase))
    rows = np.arange(height)[:, None] - np.cos(angle) * moved  # a pixel shows what is `moved` px behind it
    cols = np.arange(width)[None, :] + np.sin(angle) * moved
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
