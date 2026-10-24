"""The `smoke` effect: soft-edged plumes of noise, stretched upward and swirled, mixed into the picture as a colour."""

from functools import partial
from typing import Literal

import numpy as np
from scipy.special import ndtr

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal, standardise
from photo_fx.ops.warp import remap


class SmokeParams(EffectParams):
    """Smoke: plumes of fractal noise, drawn out upward and swirled, mixed into the picture as `color`."""

    amount: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How opaque the smoke is at the heart of a plume: the share of the smoke colour mixed into the picture "
        "there. 0 = no smoke, 0.5 = see-through smoke, 1 = solid colour hiding the picture.",
    )
    color: Color = param(
        "#e6e6e6",
        doc="The colour of the smoke: '#rrggbb', '#rgb' or [r, g, b] 0..1. Pale grey is steam or light smoke, a dark "
        "grey or black a heavy, sooty plume.",
    )
    coverage: float = param(
        0.45,
        unit="fraction",
        min=0,
        max=1,
        doc="About the share of the picture that has any smoke in it: 0 = none, 0.45 = plumes with clear air between "
        "them, 1 = smoke everywhere, in thicker and thinner places. The plumes are the highest parts of a noise "
        "field, so raising it widens them and joins them up.",
    )
    feather: float = param(
        0.25,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How soft the edge of a plume is, as the share of the picture over which the smoke fades from nothing "
        "to full `amount` just above the coverage cut. Small = crisp, billowing outlines; large = plumes that melt "
        "into the air with no clear outline.",
    )
    scale: float = param(
        100.0,
        unit="px",
        min=1,
        max=1024,
        doc="Size of the biggest plumes across, in pixels, the spacing of the first noise lattice before `rise` "
        "stretches it; tuned for a picture about 480 px high. Larger = a few big clouds; small = many small wisps.",
    )
    rise: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=8,
        doc="How much the plumes are drawn out upward, anchored at the bottom edge: 1 = round puffs, 2 = twice as "
        "tall as wide, 8 = long columns, like smoke from a chimney.",
    )
    turbulence: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels two more noise fields (with the same `scale`, `octaves`, `persistence` and "
        "`lacunarity`) push the plumes sideways and up and down: the typical displacement (their standard "
        "deviation). 0 = smooth, regular plumes; larger = twisting, curling smoke; much larger than `scale` tears "
        "the plumes into scattered wisps.",
    )
    octaves: int = param(
        4,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed to make the plumes, each finer than the last (by `lacunarity`) and "
        "weaker (by `persistence`). 1 = smooth blobs, more = more wispy detail. Layers finer than one pixel are "
        "dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before: each layer's weight is this to the power of its "
        "number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = "
        "every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
    )
    resampling: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How the noise is read where `rise` and `turbulence` have moved it between pixels: 'nearest' takes the "
        "closest value (fastest, a little jagged), 'bilinear' blends the four around it (smooth), 'bicubic' fits a "
        "cubic spline through the neighbours (smoothest, a little slower). It matters only where something moves. "
        "Named `resampling`, not `interpolation`, because it reads a generated noise field, never the picture.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the noise is beyond the picture's border when `turbulence` pushes a plume in from outside: "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same smoke, another seed different plumes.",
    )


@effect(category="atmosphere", params=SmokeParams)
def smoke(rgb: np.ndarray, params: SmokeParams) -> np.ndarray:
    """Mix soft-edged plumes of `color` into the picture, up to `amount` thick.

    The plumes are the high parts of a fractal noise field (`scale`, `octaves`, `persistence`, `lacunarity`), warped
    when it is read (`resampling`, `edge`): a pixel the height d above the bottom edge reads the field at d / `rise`
    (so the plumes are drawn out `rise` times upward from the bottom edge), pushed `turbulence` px along two more
    noise fields. The warped field is standardised and passed through the normal cumulative distribution, giving u
    spread evenly over 0..1; the opacity is `amount` x smoothstep((u - (1 - `coverage`)) / `feather`), so about
    `coverage` of the picture has smoke and the opacity climbs from 0 to `amount` across the `feather` share of
    pixels above the cut. Each pixel becomes picture + opacity x (color - picture).
    """
    height, width = rgb.shape[:2]
    noise = partial(
        fractal,
        (height, width),
        scale=params.scale,
        octaves=params.octaves,
        persistence=params.persistence,
        lacunarity=params.lacunarity,
    )
    # constant: one stream per field, numbered from 0 (the seed itself)
    field, sway_down, sway_across = (noise(seed=[params.seed, stream]) for stream in range(3))
    rows = (height - 1) * (1 - 1 / params.rise) + np.arange(height)[:, None] / params.rise
    cols = np.arange(width)[None, :]
    warped = remap(
        ndtr(field)[..., None],  # in 0..1, the range `remap` keeps
        rows + params.turbulence * sway_down,
        cols + params.turbulence * sway_across,
        interpolation=params.resampling,
        edge=params.edge,
    )[..., 0]
    lift = np.clip((ndtr(standardise(warped)) - (1 - params.coverage)) / params.feather, 0.0, 1.0)
    # constant: 3t^2 - 2t^3 is the standard smoothstep Hermite curve
    opacity = params.amount * lift * lift * (3.0 - 2.0 * lift)
    return (rgb + opacity[..., None] * (np.asarray(params.color, np.float32) - rgb)).astype(np.float32)
