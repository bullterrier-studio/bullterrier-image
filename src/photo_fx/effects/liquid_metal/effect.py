"""The `liquid_metal` effect: the picture's brightness as a relief, polished into tinted chrome."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.blur import gaussian_blur
from photo_fx.ops.color import luminance
from photo_fx.ops.relief import slopes


class LiquidMetalParams(EffectParams):
    """Liquid metal: the brightness, smoothed by `smoothing`, as a polished relief `relief` px tall that mirrors a sky
    and a ground, coloured as a metal of `tint`."""

    smoothing: float = param(
        4.0,
        unit="px",
        min=0,
        max=20,
        doc="Gaussian sigma in px with which the brightness is smoothed before it is read as height: 0 = every pixel "
        "is a bump and the metal is rough with the picture's texture and noise, larger = broad, flowing swells.",
    )
    relief: float = param(
        60.0,
        unit="px",
        min=0,
        max=200,
        doc="How tall the relief is: a white pixel stands this many px above a black one, so the slope between "
        "neighbours is their brightness difference times this. 0 = a flat mirror (one tint-coloured picture when the "
        "horizon is at 0.5), larger = steeper relief and a more contrasty, more broken-up reflection.",
    )
    horizon: float = param(
        0.5,
        unit="fraction",
        min=0.05,
        max=0.95,
        doc="Where the environment's horizon is, as a fraction of the way from the ground (0, looking straight down) "
        "to the sky (1, looking straight up), kept off the extremes so there is always some of both; a flat surface "
        "looks at 0.5. The metal is exactly `tint` at the horizon, "
        "darker towards the ground and lighter towards the sky: raising it turns more of the reflections dark, "
        "lowering it more of them light.",
    )
    tint: Color = param(
        "#c9d6ff",
        doc="The metal's colour as it looks at the horizon ('#rrggbb', '#rgb' or [r, g, b] 0..1): the metal runs from "
        "black on the ground through this to white in the sky. A light bluish silver by default; gold is '#e6b84d'.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the smoothing Gaussian is cut off, in sigmas (only matters when `smoothing` > 0). Larger is more "
        "exact and slower.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel "
        "repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge, so a tileable picture polishes without a seam).",
    )


@effect(category="stylised", params=LiquidMetalParams)
def liquid_metal(rgb: np.ndarray, params: LiquidMetalParams) -> np.ndarray:
    """Chrome that follows the picture's brightness: its relief, `relief` px tall, mirrors a sky above a ground.

    The height is the Rec. 709 luminance smoothed by `smoothing`; only brightness matters, the picture's hues are
    gone. A viewer straight above sees, in a surface with slopes (rise_y, rise_x) down and across the picture, the
    environment at t = 0.5 + rise_y / (1 + rise_x^2 + rise_y^2) from the ground (0) to the sky (1), 0.5 where the
    surface is flat. The environment runs linearly from 0 on the ground to 0.5 at `horizon` to 1 at the sky, and the
    metal colours it from black through `tint` to white. The result stays within 0..1 by construction.
    """
    surface = gaussian_blur(luminance(rgb), params.smoothing, truncate=params.truncate, edge=params.edge)
    rise_y, rise_x = slopes(surface, scale=params.relief, edge=params.edge)
    # The reflected ray's upward part, 2 n_y n_z in -1..1, as a height in 0..1.
    looks_up = 0.5 + rise_y / (1 + rise_x**2 + rise_y**2)
    horizon = params.horizon
    # The metal's place on its black (0), tint (0.5), white (1) gradient: linear ground to horizon to sky.
    shine = np.where(looks_up < horizon, 0.5 * looks_up / horizon, 0.5 + 0.5 * (looks_up - horizon) / (1 - horizon))
    shine = shine[..., None]
    tint = np.asarray(params.tint, np.float32)
    return (tint * np.minimum(2 * shine, 1) + (1 - tint) * np.maximum(2 * shine - 1, 0)).astype(np.float32)
