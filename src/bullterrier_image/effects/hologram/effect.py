"""The `hologram` effect: the picture as projected light in one glowing colour, with edge light and flicker."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.edges import edge_strength
from bullterrier_image.ops.light import screen


class HologramParams(EffectParams):
    """A hologram: the picture's brightness shown as light of one `color`, outlined in light where it changes quickly,
    cut by scanlines and dimmed by bands of flicker that roll down it."""

    color: Color = param(
        "#30e0ff",
        doc="The colour of the projected light, which the picture's brightness and its edges are shown in: '#rrggbb', "
        "'#rgb' or [r, g, b] 0..1. A cyan is the classic hologram.",
    )
    tint: float = param(
        0.85,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the picture is replaced by its brightness as light of `color`: 0 = the picture's own colours, 1 = "
        "only the colour, bright where the picture is bright and dark where it is dark.",
    )
    glow: float = param(
        5.0,
        unit="ratio",
        min=0,
        max=50,
        doc="How brightly the edges shine: the steepness of the picture's brightness (its change per pixel, about 0.1 "
        "at a clear edge) times this is the light of `color` screened over the picture there, so it lights both "
        "sides of an edge. 0 = no edge light, and a flat area never has any; raising it makes edges brighter and "
        "more of the weaker ones show.",
    )
    scanlines: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="How dark the gaps between the scanlines are: the picture is darkened between the lines by up to this "
        "share, smoothly, so a line is brightest in its middle. 0 = no scanlines, 1 = black between them.",
    )
    flicker: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="How dim the dimmest rows of the flicker are: bands of rows are darkened, smoothly, by up to this share, "
        "and `phase` says where on the picture they are. 0 = no flicker, 1 = black at the dimmest.",
    )
    phase: float = param(
        0.0,
        unit="cycles",
        min=0,
        max=1,
        doc="Where the flicker's bands are in their cycle: they roll down the picture by their own height over one "
        "cycle, so 0.25 is a quarter of the way and 1 is back at 0. It only matters when `flicker` is above 0.",
    )
    scanline_spacing: float = param(
        3.0,
        unit="px",
        min=2,
        max=32,
        doc="The distance in px from one scanline to the next, tuned for a picture about 480 px high: lines are "
        "centred on rows 0, spacing, 2 x spacing... and the gaps between them lie half a spacing away. A larger "
        "spacing is fewer, wider-set lines.",
        advanced=True,
    )
    flicker_period: float = param(
        160.0,
        unit="px",
        min=2,
        max=2000,
        doc="The height in px of one band of the flicker, from one dim row to the next, tuned for a picture about 480 "
        "px high: the flicker repeats every this many rows. A larger period is fewer, wider bands.",
        advanced=True,
    )
    edge_sigma: float = param(
        1.5,
        unit="px",
        min=0,
        max=20,
        doc="Gaussian sigma in px of the smoothing the edges are found after, tuned for a picture about 480 px high: "
        "larger ignores fine detail and noise and gives wider, softer outlines; 0 finds every pixel's edge.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the edge smoothing's Gaussian is cut off, in sigmas: it reaches truncate x `edge_sigma` px. Larger "
        "is more exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the edge finding sees beyond the picture's border: 'nearest' (the edge pixel repeated, so the "
        "border is no edge), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), "
        "'wrap' (the opposite edge, so a different brightness there makes an edge).",
        advanced=True,
    )


@effect(category="glitch", params=HologramParams)
def hologram(rgb: np.ndarray, params: HologramParams) -> np.ndarray:
    """Show the picture as light of `color`, outlined where its brightness changes, with scanlines and a flicker.

    In this order: the picture is mixed by `tint` with its Rec. 709 brightness times `color`; light of `color` x `glow`
    x the steepness of the brightness (`ops.edges`, smoothed by `edge_sigma`) is screened over it, so it never
    darkens; each row is darkened by `scanlines` x sin^2(pi x row / `scanline_spacing`) and by `flicker` x (1 - cos(2 pi
    x (row / `flicker_period` - `phase`))) / 2, so the bands roll down by their period over one cycle of `phase`.
    """
    height = rgb.shape[0]
    brightness = luminance(rgb)
    light = np.asarray(params.color, np.float32)
    shown = rgb * (1 - params.tint) + params.tint * brightness[..., None] * light
    edges = edge_strength(brightness, sigma=params.edge_sigma, truncate=params.truncate, edge=params.edge)
    lit = screen(shown, params.glow * edges[..., None] * light)
    rows = np.arange(height)
    scanline_gain = 1 - params.scanlines * np.sin(np.pi * rows / params.scanline_spacing) ** 2
    # constant: (1 - cos x) / 2 is 0 at x = 0 and rises smoothly to 1 at half a turn, the shape of one band
    band = (1 - np.cos(2 * np.pi * (rows / params.flicker_period - params.phase))) / 2
    flicker_gain = 1 - params.flicker * band
    return (lit * (scanline_gain * flicker_gain).astype(np.float32)[:, None, None]).astype(np.float32)
