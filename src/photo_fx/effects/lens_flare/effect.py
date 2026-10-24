"""The `lens_flare` effect: a halo, a ring and ghosts of one light, as stray light inside a lens draws them."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import screen


class LensFlareParams(EffectParams):
    """A lens flare: a halo and a ring round the light at (`x`, `y`) and `ghosts` soft discs along the line from it
    through the picture's centre, screened over the picture in `color`."""

    x: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel "
        "column, 0.5 = the middle, 1 = the last.",
    )
    y: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = "
        "the middle, 1 = the last.",
    )
    ghosts: int = param(
        5,
        unit="count",
        min=0,
        max=12,
        doc="How many ghosts (soft discs, the lens's reflections of the light) lie along the line from the light "
        "through the picture's centre; each is dimmer than the one before, the last 1/ghosts as bright as the "
        "first. 0 = none.",
    )
    spacing: float = param(
        0.45,
        unit="ratio",
        min=0,
        max=3,
        doc="Where the ghosts lie on that line, as multiples of the distance from the light to the picture's "
        "centre: ghost number i sits i x spacing of the way from the light toward the centre. 1 puts the first on "
        "the centre, 2 mirrors the light through it, more goes beyond; 0 piles them on the light. A light at the "
        "centre gives them no line, so they sit on it.",
    )
    size: float = param(
        0.05,
        unit="fraction",
        min=0.002,
        max=0.5,
        doc="The radius of each ghost as a fraction of the picture's diagonal: the disc is brightest at its middle "
        "and falls smoothly to nothing at this distance. Raising it makes bigger, softer ghosts.",
    )
    halo: float = param(
        0.2,
        unit="fraction",
        min=0,
        max=1,
        doc="The radius of the soft glow round the light, as a fraction of the picture's diagonal: brightest at the "
        "light and falling smoothly to nothing at this distance. 0 = no halo.",
    )
    ring: float = param(
        0.12,
        unit="fraction",
        min=0,
        max=1,
        doc="The radius of the thin ring round the light, as a fraction of the picture's diagonal: the distance from "
        "the light at which the ring is brightest. 0 = no ring.",
    )
    ring_width: float = param(
        0.02,
        unit="fraction",
        min=0.002,
        max=0.5,
        doc="How far each side of its radius the ring reaches, as a fraction of the picture's diagonal: it falls "
        "smoothly from full brightness on the radius to nothing this far off. Raising it makes a wider, softer ring.",
    )
    amount: float = param(
        0.8,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the flare is screened over the picture: 0 = off, 1 = the light at full brightness where "
        "it is brightest (the light itself and the first ghost); above 1 it saturates to white sooner.",
    )
    color: Color = param(
        "#ffe2b8",
        doc="The tint of the flare: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm white; white keeps "
        "the flare colourless.",
    )


def _paint(light: np.ndarray, centre: tuple[float, float], reach: float, weight: float, ring: float) -> None:
    """Add a smooth bump of peak `weight` to `light` (height, width), in place: round `centre` (row, column) its
    brightness is (1 - (d / reach)^2)^2 at distance d px, 0 from `reach` out; with a `ring` radius > 0 it is round
    the circle of that radius instead, (1 - ((d - ring) / reach)^2)^2 within `reach` of it."""
    row, col = centre
    reach_out = reach + ring
    height, width = light.shape
    rows = slice(max(int(np.floor(row - reach_out)), 0), min(int(np.ceil(row + reach_out)), height))
    cols = slice(max(int(np.floor(col - reach_out)), 0), min(int(np.ceil(col + reach_out)), width))
    if rows.stop <= rows.start or cols.stop <= cols.start:
        return  # the bump lies wholly outside the picture
    down, right = np.mgrid[rows, cols].astype(np.float64)
    away = (np.hypot(down - row, right - col) - ring) / reach
    light[rows, cols] += (weight * np.clip(1.0 - away**2, 0.0, 1.0) ** 2).astype(np.float32)


@effect(category="light", params=LensFlareParams)
def lens_flare(rgb: np.ndarray, params: LensFlareParams) -> np.ndarray:
    """A lens flare: a halo and a ring round the light at (`x`, `y`) and `ghosts` discs along the line through the
    picture's centre, tinted `color` and screened over the picture by `amount`.

    Every feature is a smooth bump of brightness, 1 where it is strongest and falling (1 - t^2)^2 to nothing at its
    radius (`halo`, `size`) or half-width (`ring_width`), drawn straight from its formula with no blur or warp. Ghost
    i is centred at light + i x `spacing` x (picture centre - light) and has the brightness (ghosts - i + 1) /
    ghosts. The picture only ever gets brighter.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    diagonal = float(np.hypot(height, width))
    light_at = np.array([params.y * (height - 1), params.x * (width - 1)])  # (row, column), in pixels
    centre = np.array([(height - 1) / 2, (width - 1) / 2])
    flare = np.zeros((height, width), np.float32)
    full = 1.0  # constant: a feature's peak brightness, before `amount` scales it
    no_ring = 0.0  # constant: the radius of a bump that is a disc and not a ring
    if params.halo > 0:
        _paint(flare, (light_at[0], light_at[1]), params.halo * diagonal, full, no_ring)
    if params.ring > 0:
        _paint(flare, (light_at[0], light_at[1]), params.ring_width * diagonal, full, params.ring * diagonal)
    for ghost in range(1, params.ghosts + 1):
        at = light_at + ghost * params.spacing * (centre - light_at)
        fade = (params.ghosts - ghost + 1) / params.ghosts
        _paint(flare, (at[0], at[1]), params.size * diagonal, fade, no_ring)
    tint = np.asarray(params.color, np.float32)
    return screen(rgb, params.amount * flare[..., None] * tint)
