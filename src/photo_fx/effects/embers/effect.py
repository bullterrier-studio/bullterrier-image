"""The `embers` effect: glowing sparks rising through the picture, each trailing a short streak."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import screen
from photo_fx.ops.particles import draw_segments


class EmbersParams(EffectParams):
    """Embers: small bright sparks drifting up, each with a streak behind it, in a glowing colour."""

    count: int = param(
        120,
        unit="count",
        min=0,
        max=2000,
        doc="How many embers there are, whatever the size of the picture (a number, not a density). 0 = none (the "
        "picture unchanged), 120 is a lively shower of sparks.",
    )
    size: float = param(
        7.0,
        unit="px",
        min=1,
        max=30,
        doc="The width in px of the nearest, largest embers; the farthest are narrower by `depth`. Tuned for a picture "
        "about 480 px high. Larger = fatter sparks.",
    )
    softness: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How soft an ember's edge is: it falls from full brightness to none across this fraction of the ember's "
        "radius (at least one pixel). 0 = crisp sparks, 1 = sparks glowing out from their middle.",
    )
    length: float = param(
        8.0,
        unit="px",
        min=0,
        max=200,
        doc="The length in px of the streak each nearest ember leaves behind it; the farthest are shorter by `depth`. "
        "Tuned for a picture about 480 px high. 0 = round dots, larger = faster-looking sparks.",
    )
    angle: float = param(
        15.0,
        unit="degrees",
        min=-45,
        max=45,
        doc="How far the embers drift from straight up, in degrees: positive drifts to the right as they rise (wind "
        "from the left), negative to the left. 0 = straight up. The streaks trail the other way.",
    )
    depth: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far apart the embers are in depth, which spreads their width, streak length and brightness: the "
        "farthest have 1 - depth times those of the nearest. 0 = every ember alike, 1 = from the nearest down to "
        "nothing at the farthest.",
    )
    opacity: float = param(
        0.9,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright the nearest embers are: the share of `color` they screen over the picture along their middle; "
        "the farthest are fainter. 0 = invisible, 1 = solid colour.",
    )
    color: Color = param(
        "#ff8a2b",
        doc="The colour of the glow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Embers are screened over the picture, so "
        "they only lighten it.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same embers in the same places, another seed others.",
    )


@effect(category="atmosphere", params=EmbersParams)
def embers(rgb: np.ndarray, params: EmbersParams) -> np.ndarray:
    """Screen `count` sparks of `color` at random places over the picture, each rising with a streak behind it.

    Every ember lies at a random distance d from 0 (far) to 1 (near): its width, streak length and brightness are
    `size`, `length` and `opacity` times 1 - `depth` x (1 - d). It rises at `angle` from vertical, so its streak runs
    from the spark down the other way, and its edge is `softness` of its radius soft. Where embers overlap the brighter
    one shows. Embers are screened over the picture, so they only lighten it; the same `seed` always gives the same
    sparks.
    """
    height, width = rgb.shape[:2]
    # constant: three random numbers per particle, its x, y and distance
    x, y, distance = np.random.default_rng(params.seed).random((params.count, 3)).T
    near = 1 - params.depth * (1 - distance)
    head = np.stack([x * width, y * height], axis=1)
    drift = np.radians(params.angle)
    tail = head + params.length * near[:, None] * np.array([-np.sin(drift), np.cos(drift)])
    radius = params.size / 2 * near
    light = draw_segments((height, width), head, tail, radius, params.opacity * near, params.softness * radius)
    return screen(rgb, light[..., None] * np.asarray(params.color, np.float32))
