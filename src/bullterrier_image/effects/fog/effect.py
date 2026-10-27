"""The `fog` effect: a veil of drifting noise-shaped cloud, thicker toward the bottom, mixed in as a colour. Fog, mist,
haze, steam and smoke are settings of it."""

from functools import partial
from typing import Literal

import numpy as np
from scipy.special import ndtr

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.noise import fractal, standardise
from bullterrier_image.ops.warp import remap


class FogParams(EffectParams):
    """Fog: fractal-noise clouds, drawn out upward and swirled, thicker toward the bottom, mixed in as `color`."""

    amount: float = param(
        0.77,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="How opaque the fog is where it is thickest (the bottom of the frame, in the densest cloud): the share of "
        "the fog colour mixed into the picture there. 0 = no fog, 0.5 = see-through, 1 = the colour itself, hiding the "
        "picture. Fog also lowers contrast, since every pixel moves toward the same colour.",
    )
    color: Color = param(
        "#cfd8e3",
        doc="The colour of the fog: '#rrggbb', '#rgb' or [r, g, b] 0..1. A pale blue-grey is a cool morning mist, "
        "white a thick cloud, a warm grey smog, a dark grey or black a heavy, sooty plume.",
    )
    gradient: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How much thicker the fog is at the bottom of the frame than at the top, as a share of its opacity: the "
        "opacity falls in a straight line from the bottom row up, to 1 - gradient of its value at the top row. 0 = "
        "the same all the way up, 1 = no fog at the very top, as for mist lying on the ground.",
    )
    coverage: float = param(
        0.93,
        unit="fraction",
        min=0,
        max=1,
        doc="About the share of the picture that has any cloud in it: 0 = none, 0.45 = clouds with clear air between "
        "them, 1 = cloud everywhere, in thicker and thinner places. The clouds are the highest parts of a noise "
        "field, so raising it widens them and joins them up.",
    )
    feather: float = param(
        1.0,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How soft the edge of a cloud is, as the share of the picture over which the fog thickens from its "
        "thinnest to full `amount` just above the coverage cut. Small = crisp, billowing outlines; 1 = clouds that "
        "melt into the air with no clear outline.",
        advanced=True,
    )
    floor: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the `amount` is left between the clouds, as a share: the opacity where there is no cloud is "
        "`amount` x `floor`. 0 = open gaps of clear air, like plumes of smoke; 1 = an even veil with no clouds at "
        "all; 0.5 = a haze that thins between drifts but never clears.",
        advanced=True,
    )
    scale: float = param(
        150.0,
        unit="px",
        min=1,
        max=1024,
        doc="Size of the biggest clouds across, in pixels, the spacing of the first noise lattice before `rise` "
        "stretches it; tuned for a picture about 480 px high. Larger = broad banks of fog; small = fine, wispy "
        "mottling.",
    )
    rise: float = param(
        1.0,
        unit="ratio",
        min=1,
        max=8,
        doc="How much the clouds are drawn out upward, anchored at the bottom edge: 1 = round puffs, 2 = twice as "
        "tall as wide, 8 = long columns, like smoke from a chimney.",
    )
    turbulence: float = param(
        0.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels two more noise fields (with the same `scale`, `octaves`, `persistence` and "
        "`lacunarity`) push the clouds sideways and up and down: the typical displacement (their standard "
        "deviation). 0 = smooth, regular clouds; larger = twisting, curling smoke; much larger than `scale` tears "
        "the clouds into scattered wisps.",
    )
    octaves: int = param(
        4,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed to make the clouds, each finer than the last (by `lacunarity`) and "
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
        advanced=True,
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = "
        "every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
        advanced=True,
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
        doc="What the noise is beyond the picture's border when `turbulence` pushes a cloud in from outside: "
        "'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge "
        "pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same clouds, another seed a different bank of fog.",
    )


@effect(category="atmosphere", params=FogParams)
def fog(rgb: np.ndarray, params: FogParams) -> np.ndarray:
    """Mix the picture toward `color` by a fog opacity that is cloudy and heavier toward the bottom.

    The clouds are the high parts of a fractal noise field (`scale`, `octaves`, `persistence`, `lacunarity`), warped
    when it is read (`resampling`, `edge`): a pixel the height d above the bottom edge reads the field at d / `rise`
    (so the clouds are drawn out `rise` times upward from the bottom edge), pushed `turbulence` px along two more
    noise fields. The warped field is standardised and passed through the normal cumulative distribution, giving u
    spread evenly over 0..1; the cloud shape is smoothstep((u - (1 - `coverage`)) / `feather`), so about `coverage` of
    the picture has cloud and the shape climbs from 0 to 1 across the `feather` share of pixels above the cut. The
    opacity at a pixel is `amount` x (1 - `gradient` x (1 - t)) x (`floor` + (1 - `floor`) x shape), where t is how
    far down the frame the pixel's row centre is (0 at the top edge, 1 at the bottom edge). Each pixel becomes
    picture + opacity x (color - picture), so the fog lowers contrast as well as tinting.
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
    cloud, sway_down, sway_across = (noise(seed=[params.seed, stream]) for stream in range(3))
    rows = (height - 1) * (1 - 1 / params.rise) + np.arange(height)[:, None] / params.rise
    cols = np.arange(width)[None, :]
    warped = remap(
        ndtr(cloud)[..., None],  # in 0..1, the range `remap` keeps
        rows + params.turbulence * sway_down,
        cols + params.turbulence * sway_across,
        interpolation=params.resampling,
        edge=params.edge,
    )[..., 0]
    lift = np.clip((ndtr(standardise(warped)) - (1 - params.coverage)) / params.feather, 0.0, 1.0)
    # constant: 3t^2 - 2t^3 is the standard smoothstep Hermite curve
    shape = lift * lift * (3.0 - 2.0 * lift)
    down = (np.arange(height) + 0.5) / height
    opacity = params.amount * (1 - params.gradient * (1 - down))[:, None] * (params.floor + (1 - params.floor) * shape)
    return (rgb + opacity[..., None] * (np.asarray(params.color, np.float32) - rgb)).astype(np.float32)
