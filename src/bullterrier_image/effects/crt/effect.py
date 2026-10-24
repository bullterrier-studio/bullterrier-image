"""The `crt` effect: the picture on a curved glass tube, with a shadow mask, scanlines, bloom and a vignette."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.light import highlights, screen
from bullterrier_image.ops.warp import Centre, remap


class CrtParams(EffectParams):
    """A cathode-ray tube: the picture bulged by the curve of the glass, lit through a shadow mask of red, green and
    blue stripes in scanlines, with the bright parts glowing and the edges of the glass dim."""

    curve: float = param(
        0.15,
        unit="ratio",
        min=0,
        max=1,
        doc="How far the glass bulges: each pixel shows the picture at 1 + curve x (d / R)^2 times its distance from "
        "the middle, d its own distance from it and R the distance of a corner (the radial term of the "
        "Brown-Conrady lens model), so straight lines bow outward, more toward the corners, and the corners and "
        "edges reach beyond the picture and show `border` instead. 0 = a flat screen; 1 = a strongly bulged one.",
    )
    mask: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="How strongly the shadow mask shows: the screen is covered with red, green and blue vertical stripes, and "
        "a stripe lets through only its own colour. 0 = no mask; at 1 each channel is lit in a third of the columns "
        "only. The mask keeps the picture's average brightness (the lit stripes are brightened by as much as the "
        "others are darkened), so raising it shows the stripes without dimming the picture.",
    )
    scanlines: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="How dark the gaps between the lines of the raster are: the picture is darkened between the lines by up "
        "to this share, smoothly, so a line is brightest in its middle. 0 = no scanlines, 1 = black between "
        "them.",
    )
    bloom: float = param(
        0.3,
        unit="ratio",
        min=0,
        max=2,
        doc="How strongly the bright parts glow into their surroundings, as light spreading in the glass: the "
        "blurred highlights are screened over the picture by this much. 0 = no glow, 1 = the halo at full "
        "brightness, above 1 it reaches white sooner.",
    )
    vignette: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="How much dimmer the screen is at its corners than in the middle: the picture is darkened in proportion "
        "to the square of the distance from the middle, up to this share at a corner. 0 = an evenly lit screen.",
    )
    border: Color = param(
        "#000000",
        doc="The colour beyond the curved screen, where the glass ends and the bezel begins: '#rrggbb', '#rgb' or "
        "[r, g, b] 0..1. It covers everything else there, and is only seen where `curve` is above 0.",
        advanced=True,
    )
    mask_pitch: int = param(
        3,
        unit="px",
        min=3,
        max=24,
        doc="The width in px of one triad of red, green and blue stripes, a third of it for each, tuned for a picture "
        "about 480 px high: 3 gives stripes one pixel wide, larger gives coarser ones.",
        advanced=True,
    )
    scanline_spacing: float = param(
        4.0,
        unit="px",
        min=2,
        max=32,
        doc="The distance in px from one line of the raster to the next, tuned for a picture about 480 px high: lines "
        "are centred on rows 0, spacing, 2 x spacing... and the gaps between them lie half a spacing away. A larger "
        "spacing is fewer, wider-set lines.",
        advanced=True,
    )
    bloom_radius: float = param(
        10.0,
        unit="px",
        min=0,
        max=100,
        doc="Gaussian sigma in px of the glow, tuned for a picture about 480 px high: larger spreads the light "
        "further; 0 keeps it on the highlights themselves.",
        advanced=True,
    )
    bloom_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1, per channel) where the glow starts: only values above it give light, rising to "
        "full at 1. Raising it limits the glow to the brightest parts.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the glow's Gaussian is cut off, in sigmas: it reaches truncate x `bloom_radius` px. Larger is "
        "more exact and slower.",
        advanced=True,
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read when the glass bends the picture: 'nearest' takes the closest pixel "
        "(fastest, but lines turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft), "
        "'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring "
        "beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the bend and the glow's blur see beyond the picture's edge: 'reflect' (mirrored, edge pixel "
        "repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge). The bend's own beyond is covered by `border`.",
        advanced=True,
    )


@effect(category="texture", params=CrtParams)
def crt(rgb: np.ndarray, params: CrtParams) -> np.ndarray:
    """Show the picture on a CRT: bulged glass, a glow, scanlines, a shadow mask, a dim edge and the bezel `border`.

    In this order: each pixel d px from the middle reads the picture at d x (1 + `curve` x (d / R)^2), R the distance
    of a corner; the highlights above `bloom_threshold` are blurred by `bloom_radius` and screened over by `bloom`;
    each row is darkened by `scanlines` x sin^2(pi x row / `scanline_spacing`); the red, green and blue stripes of
    the mask (a triad every `mask_pitch` px) pass their own channel fully and the others by 1 - `mask`, the
    gains divided by their mean; the picture is darkened by `vignette` x (d / R)^2; and what the bend read from
    beyond the picture is `border` (blended in over the last pixel). The mask can brighten a lit stripe
    above 1; the result is clipped to 0..1. With every part off the picture is returned as it is.
    """
    height, width = rgb.shape[:2]
    centre = Centre.middle(height, width)
    farthest = float(np.hypot(height - 1, width - 1)) / 2  # the corner's distance from the middle, in px
    if farthest == 0:
        return rgb.copy()
    share = (centre.distance / farthest) ** 2  # 0 in the middle, 1 at a corner
    rows, cols = centre.sources(scale=1 + params.curve * share)
    out = remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)

    halo = gaussian_blur(
        highlights(out, params.bloom_threshold), params.bloom_radius, truncate=params.truncate, edge=params.edge
    )
    out = screen(out, params.bloom * halo)

    row_gain = 1 - params.scanlines * np.sin(np.pi * np.arange(height) / params.scanline_spacing) ** 2
    out = out * row_gain.astype(np.float32)[:, None, None]

    channels = out.shape[-1]
    stripe = (np.arange(width) % params.mask_pitch) * channels // params.mask_pitch  # which channel a column lights
    lit = stripe[:, None] == np.arange(channels)
    column_gain = 1 - params.mask * (1 - lit)
    out = out * (column_gain / column_gain.mean()).astype(np.float32)[None]

    out = out * (1 - params.vignette * share).astype(np.float32)[..., None]

    # reading from less than a pixel beyond the first or last pixel blends it into the border; farther out is all border
    inside_rows = np.clip(np.minimum(rows + 1, height - rows), 0, 1)
    inside_cols = np.clip(np.minimum(cols + 1, width - cols), 0, 1)
    glass = (inside_rows * inside_cols).astype(np.float32)[..., None]
    return np.clip(out * glass + np.asarray(params.border, np.float32) * (1 - glass), 0.0, 1.0).astype(np.float32)
