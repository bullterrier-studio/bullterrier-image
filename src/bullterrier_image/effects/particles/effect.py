"""The `particles` effect: dots and streaks of light at different depths, as rain, snow, embers, dust or stars."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.light import screen
from bullterrier_image.ops.particles import draw_segments


class ParticlesParams(EffectParams):
    """Particles: bright dots or streaks at random places and depths, the near ones large and bright."""

    density: float = param(
        1.0,
        unit="count",
        min=0,
        max=100,
        doc="How many particles there are: particles per 1000 square px of picture (a 32 x 32 px area), so the same "
        "density looks alike on any picture size. 0 = none (the picture unchanged), 1 is a steady shower of rain, 7 a "
        "clear night sky of stars.",
    )
    size: float = param(
        6.0,
        unit="px",
        min=1,
        max=200,
        doc="The width in px of the nearest, largest particles; farther ones are narrower by `size_falloff`. Tuned for "
        "a picture about 480 px high. Larger = bigger dots and thicker streaks.",
    )
    length: float = param(
        0.0,
        unit="px",
        min=0,
        max=200,
        doc="The length in px of the streak each nearest particle draws from its head along `angle`; farther ones are "
        "shorter by `size_falloff`. Tuned for a picture about 480 px high. 0 = round dots, larger = longer streaks, "
        "as with a slower shutter.",
    )
    angle: float = param(
        90.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction a streak runs from its head, in degrees clockwise from pointing right, as seen on screen: "
        "0 = right, 90 = down, 180 = left, 270 = up. Only streaks show it: with a `length` of 0 it changes nothing.",
    )
    softness: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How soft a particle's edge is: it falls from full brightness to none across this fraction of the "
        "particle's radius (at least one pixel), so large ones are blurrier than small ones, like particles out of "
        "focus. 0 = crisp one-pixel edges, 1 = fading out from the middle.",
    )
    color: Color = param(
        "#ffffff",
        doc="The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. Particles are screened over the picture, "
        "so they only lighten it.",
    )
    layers: int = param(
        0,
        unit="count",
        min=0,
        max=10,
        advanced=True,
        doc="How many depth planes the particles are in. In plane k of n the particle is k / n of the way from the "
        "farthest to the nearest, so more planes = a smoother range of depths, and 1 = every particle at the nearest "
        "depth. 0 = no planes, every depth between the nearest and the farthest occurs.",
    )
    size_falloff: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        advanced=True,
        doc="How much smaller the farthest particles are: their width and length are 1 - this of the nearest ones'. "
        "0 = every particle the same size, 1 = from the nearest down to nothing at the farthest.",
    )
    brightness_falloff: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        advanced=True,
        doc="How much fainter the farthest particles are: their brightness is 1 - this of the nearest ones'. 0 = every "
        "particle equally bright, 1 = from the nearest down to nothing at the farthest.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same particles in the same places, another seed others.",
    )


@effect(category="atmosphere", params=ParticlesParams, opacity=0.8)
def particles(rgb: np.ndarray, params: ParticlesParams) -> np.ndarray:
    """Screen `density` particles of `color` per 1000 px² at random places, each a dot or a streak along `angle`.

    Every particle lies at a random nearness d from 0 (farthest) to 1 (nearest), or in one of `layers` planes k / n.
    Its width and streak length are `size` and `length` times 1 - `size_falloff` x (1 - d), and it screens
    1 - `brightness_falloff` x (1 - d) of `color` along its middle, with an edge `softness` of its radius soft, so the
    picture has depth. Where particles overlap the brighter one shows; they are cut off by the picture's edge and
    only lighten it. A higher `density` keeps every particle of a lower one and adds more; the same `seed` always
    gives the same particles.
    """
    height, width = rgb.shape[:2]
    # constant: density is counted per 1000 square px
    # constant: three random numbers per particle, its x, y and nearness
    x, y, nearness = np.random.default_rng(params.seed).random((round(params.density * height * width / 1000), 3)).T
    if params.layers:
        nearness = (np.floor(nearness * params.layers) + 1) / params.layers
    scale = 1 - params.size_falloff * (1 - nearness)
    head = np.stack([x * width, y * height], axis=1)
    heading = np.radians(params.angle)
    tail = head + params.length * scale[:, None] * np.array([np.cos(heading), np.sin(heading)])
    radius = params.size / 2 * scale
    layer = draw_segments(
        (height, width), head, tail, radius, 1 - params.brightness_falloff * (1 - nearness), params.softness * radius
    )
    return screen(rgb, layer[..., None] * np.asarray(params.color, np.float32))
