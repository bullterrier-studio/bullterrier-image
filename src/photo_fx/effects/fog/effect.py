"""The `fog` effect: layered haze that thickens toward the bottom of the frame, mixing the picture toward a colour."""

import numpy as np
from scipy.special import ndtr

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal


class FogParams(EffectParams):
    """Fog: the picture mixed toward `color` by an opacity of at most `density`, cloudy, thickest at the bottom."""

    density: float = param(
        0.8,
        unit="fraction",
        min=0,
        max=1,
        doc="How thick the fog is where it is thickest (the bottom of the frame, in the densest cloud): the share of "
        "the fog colour mixed into the picture there. 0 = no fog, 0.8 = a heavy haze, 1 = the colour itself, "
        "hiding the picture. Fog also lowers contrast, since every pixel moves toward the same colour.",
    )
    color: Color = param(
        "#cfd8e3",
        doc="The colour of the fog: '#rrggbb', '#rgb' or [r, g, b] 0..1. A pale blue-grey is a cool morning mist, "
        "white a thick cloud, a warm grey smog.",
    )
    gradient: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How much thicker the fog is at the bottom of the frame than at the top, as a share of its density: the "
        "opacity falls in a straight line from the bottom row up, to 1 - gradient of its value at the top row. 0 = "
        "the same all the way up, 1 = no fog at the very top, as for mist lying on the ground.",
    )
    patchiness: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=1,
        doc="How cloudy the fog is: 0 = an even veil, 1 = drifts with clear gaps, where the noise is lowest the fog "
        "thins to nothing. The cloud shapes come from fractal noise (`scale`, `octaves`, `persistence`, "
        "`lacunarity`, `seed`).",
    )
    scale: float = param(
        150.0,
        unit="px",
        min=1,
        max=1024,
        doc="Size of the biggest clouds in pixels, the spacing of the first noise lattice; tuned for a picture about "
        "480 px high. Larger = broad banks of fog; small = fine, wispy mottling.",
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
        doc="How much weaker each finer layer of cloud is than the one before: each layer's weight is this to the "
        "power of its number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer of cloud is than the one before: the lattice spacing is divided by this per "
        "layer. 1 = every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
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

    The opacity at a pixel is `density` x (1 - `gradient` x (1 - t)) x (1 - `patchiness` x (1 - c)), where t is how
    far down the frame the pixel's row centre is (0 at the top edge, 1 at the bottom edge) and c in 0..1 is the
    cloud: fractal noise (`scale`, `octaves`, `persistence`, `lacunarity`, `seed`) passed through the normal
    cumulative distribution, so c is spread evenly over 0..1 whatever the noise settings. Each pixel becomes
    picture + opacity x (color - picture), so the fog lowers contrast as well as tinting.
    """
    height, width = rgb.shape[:2]
    cloud = ndtr(
        fractal(
            (height, width),
            scale=params.scale,
            octaves=params.octaves,
            persistence=params.persistence,
            lacunarity=params.lacunarity,
            seed=params.seed,
        )
    )
    down = (np.arange(height) + 0.5) / height
    opacity = params.density * (1 - params.gradient * (1 - down))[:, None] * (1 - params.patchiness * (1 - cloud))
    return (rgb + opacity[..., None] * (np.asarray(params.color, np.float32) - rgb)).astype(np.float32)
