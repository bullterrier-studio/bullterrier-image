"""The `halftone` effect: a print-style dot screen, where tone becomes dot size."""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import ink_amount, luminance, vivid


class HalftoneParams(EffectParams):
    """A dot screen: a rotated square grid of dots, each as large as the ink its patch of the picture needs."""

    cell: int = param(
        14,
        unit="px",
        min=1,
        max=200,
        doc="Distance between neighbouring dots in px (the screen's pitch). Larger = fewer, bigger dots, a coarser "
        "print.",
    )
    angle: float = param(
        45.0,
        unit="degrees",
        min=-360,
        max=360,
        doc="Rotation of the dot grid, counter-clockwise in degrees; the grid is square, so 90 degrees on looks the "
        "same. 45 is the classic screen angle for one ink.",
    )
    fg: Color | None = param(
        None,
        doc="The ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each "
        "dot the colour of the photo under it, brightened so small dots stay visible.",
    )
    bg: Color = param(
        "#000000",
        doc="The paper colour between the dots. On a dark paper bright tones get the ink; on a light one, dark tones.",
    )
    sigma: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=5,
        doc="Gaussian sigma, in cells, of the blur that averages the tone each dot stands for: 0 = each dot sees only "
        "the pixel at its centre (detail, but noisy), larger = smoother tone that spreads into neighbouring dots.",
    )
    feather: float = param(
        1.0,
        unit="px",
        min=0.1,
        max=50,
        doc="Width of each dot's soft edge in px (anti-aliasing): small = crisp dots, larger = blurrier dots. The ink "
        "stays the same.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the tone blur's Gaussian is cut off, in sigmas: it reaches truncate x sigma cells. Larger is more "
        "exact and slower.",
    )
    mode: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the tone blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), "
        "'constant' (constant_value).",
    )
    constant_value: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) assumed beyond the edge when mode is 'constant'; ignored otherwise.",
    )


@effect(category="stylised", params=HalftoneParams)
def halftone(rgb: np.ndarray, params: HalftoneParams) -> np.ndarray:
    """Print-style dot screen: tone becomes dot size. `cell` = distance between dots in px, `angle` = screen rotation.

    A dot's area is proportional to the ink its tone needs, so the screen keeps the picture's tones; dots touch at
    half tone and fill their cell at full tone. `fg` = one colour for every dot; leave it out to take each dot's
    colour from the photo.
    """
    height, width = rgb.shape[:2]
    cell = params.cell
    paper = np.asarray(params.bg, dtype=np.float32)
    smooth = np.stack(
        [
            ndimage.gaussian_filter(
                rgb[..., c],
                cell * params.sigma,
                mode=params.mode,
                cval=params.constant_value,
                truncate=params.truncate,
            )
            for c in range(3)
        ],
        axis=-1,
    )

    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    cos, sin = np.cos(np.radians(params.angle)), np.sin(np.radians(params.angle))
    u, v = (x * cos + y * sin) / cell, (-x * sin + y * cos) / cell  # position in the rotated dot grid, in cells
    cu, cv = np.floor(u) + 0.5, np.floor(v) + 0.5  # centre of the dot this pixel belongs to
    dist = np.hypot(u - cu, v - cv)

    # Look the picture up at the dot's centre, mapped back to image coordinates.
    sx = np.clip(np.rint((cu * cos - cv * sin) * cell), 0, width - 1).astype(int)
    sy = np.clip(np.rint((cu * sin + cv * cos) * cell), 0, height - 1).astype(int)
    sampled = smooth[sy, sx]

    # A dot's area follows the ink. At full ink it is the circle around its square cell, radius half the diagonal,
    # so it covers the cell; dots touch their neighbours at half ink.
    full_ink_radius = 0.7071  # constant: sqrt(2) / 2 to four places, the half-diagonal of a unit cell
    radius = np.sqrt(np.clip(ink_amount(luminance(sampled), paper), 0, 1)) * full_ink_radius
    coverage = np.clip((radius - dist) * cell / params.feather + 0.5, 0.0, 1.0)[..., None]  # ramp centred on the edge
    dots = vivid(sampled) if params.fg is None else np.asarray(params.fg, dtype=np.float32)
    return (paper * (1.0 - coverage) + dots * coverage).astype(np.float32)
