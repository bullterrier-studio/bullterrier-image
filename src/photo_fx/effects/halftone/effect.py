"""The `halftone` effect: a print-style dot screen, where tone becomes dot size."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.color import ink_amount, luminance, vivid

# constant: pixels drawn at once; bounds the working memory and never changes the result
_BAND_PIXELS = 2**16


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
        doc="Rotation of the dot grid in degrees, clockwise on screen: rows of dots run that many degrees below the "
        "x axis (negative = above). The grid is square, so 90 degrees on looks the same; 45 is the classic screen "
        "angle for one ink.",
    )
    ink: Color | None = param(
        None,
        doc="The ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each "
        "dot the colour of the photo under it, brightened so small dots stay visible (by at most 1 / `ink_floor`).",
    )
    ink_floor: float = param(
        0.35,
        unit="fraction",
        min=0.01,
        max=1,
        doc="With no `ink`: the most a dot's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a "
        "colour whose strongest channel is below it is scaled by 1 / ink_floor instead of up to full, so dark areas "
        "stay dark. Lowering it brightens dark colours more; 1 = colours as they are.",
    )
    paper: Color = param(
        "#000000",
        doc="The paper colour between the dots. On a dark paper (Rec. 709 luminance below `paper_threshold`) bright "
        "tones get the ink; on a light one, dark tones.",
    )
    paper_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (bright tones get the ink) rather "
        "than light (dark tones get it); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every "
        "paper as light, 1 every paper but white as dark.",
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
    edge: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
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
        doc="The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.",
    )


@effect(category="stylised", params=HalftoneParams)
def halftone(rgb: np.ndarray, params: HalftoneParams) -> np.ndarray:
    """Print-style dot screen: tone becomes dot size. `cell` = distance between dots in px, `angle` = screen rotation.

    A dot's area is proportional to the ink its tone needs, so the screen keeps the picture's tones; dots touch at
    half tone and fill their cell at full tone. `ink` = one colour for every dot; leave it out to take each dot's
    colour from the photo.
    """
    height, width = rgb.shape[:2]
    cell = params.cell
    paper = np.asarray(params.paper, dtype=np.float32)
    smooth = gaussian_blur(
        rgb, cell * params.sigma, truncate=params.truncate, edge=params.edge, constant_value=params.constant_value
    )
    cos, sin = np.cos(np.radians(params.angle)), np.sin(np.radians(params.angle))
    x = np.arange(width, dtype=np.float32)
    ink = None if params.ink is None else np.asarray(params.ink, dtype=np.float32)
    # The radius of a dot at full ink: half the diagonal of its square cell, so dots touch their neighbours at half ink.
    full_ink_radius = 0.7071  # constant: sqrt(2) / 2 to four places, the half-diagonal of a unit cell
    screen = np.empty(smooth.shape, dtype=np.float32)
    # Every pixel of the screen depends only on its own position and the blurred picture, so it is drawn a band of
    # rows at a time: the same result as all at once, with the temporaries of one band instead of the whole picture.
    rows = max(1, _BAND_PIXELS // width)
    for top in range(0, height, rows):
        y = np.arange(top, min(top + rows, height), dtype=np.float32)[:, None]
        u, v = (x * cos + y * sin) / cell, (-x * sin + y * cos) / cell  # position in the rotated dot grid, in cells
        cu, cv = np.floor(u) + 0.5, np.floor(v) + 0.5  # centre of the dot this pixel belongs to
        dist = np.hypot(u - cu, v - cv)

        # Look the picture up at the dot's centre, mapped back to image coordinates.
        sx = np.clip(np.rint((cu * cos - cv * sin) * cell), 0, width - 1).astype(int)
        sy = np.clip(np.rint((cu * sin + cv * cos) * cell), 0, height - 1).astype(int)
        sampled = smooth[sy, sx]

        # A dot's area follows the ink: at full ink it is the circle around its square cell, so it covers the cell.
        radius = (
            np.sqrt(np.clip(ink_amount(luminance(sampled), paper, threshold=params.paper_threshold), 0, 1))
            * full_ink_radius
        )
        # The soft edge is a ramp `feather` px wide centred on the dot's edge. A dot narrower than the ramp would
        # still print its inner half (half strength at the centre even at zero ink), so the ramp fades in with the
        # dot: in full once the radius reaches half the feather, gone at zero. Zero ink is bare paper, and a tiny dot
        # prints about its own area.
        fade_in = np.clip(radius * cell / (params.feather / 2), 0.0, 1.0)
        coverage = (np.clip((radius - dist) * cell / params.feather + 0.5, 0.0, 1.0) * fade_in)[..., None]
        dots = vivid(sampled, floor=params.ink_floor) if ink is None else ink
        screen[top : top + rows] = paper * (1.0 - coverage) + dots * coverage
    return screen
