"""The `rain` effect: falling streaks of light screened over the picture."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.light import screen
from bullterrier_image.ops.particles import draw_segments


class RainParams(EffectParams):
    """Rain: thin bright streaks falling at an angle, near ones big and bright, far ones small and faint."""

    density: float = param(
        1.0,
        unit="count",
        min=0,
        max=5,
        doc="How many streaks fall: streaks per 1000 square px of picture (a 32 x 32 px area), so the same density "
        "looks alike on any picture size. 0 = none (the picture unchanged), 1 is a steady shower, 3 a downpour.",
    )
    angle: float = param(
        10.0,
        unit="degrees",
        min=-45,
        max=45,
        doc="How far the streaks lean from straight down, in degrees: positive leans toward the right as they fall "
        "(wind from the left), negative toward the left. 0 = vertical.",
    )
    length: float = param(
        32.0,
        unit="px",
        min=1,
        max=200,
        doc="The length of the nearest streaks in px; the farthest are shorter by `depth`. Tuned for a picture about "
        "480 px high. Larger = longer streaks, as with a slower shutter.",
    )
    thickness: float = param(
        1.5,
        unit="px",
        min=1,
        max=20,
        doc="The width of the nearest streaks in px; the farthest are narrower by `depth`. Tuned for a picture about "
        "480 px high. Larger = fatter drops.",
    )
    depth: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far apart the streaks are in depth, which spreads their length, width and brightness: the farthest "
        "have 1 - depth times those of the nearest. 0 = every streak alike, 1 = from the nearest down to nothing at "
        "the farthest.",
    )
    color: Color = param(
        "#dbe6f5",
        doc="The colour of the rain: '#rrggbb', '#rgb' or [r, g, b] 0..1. Streaks are screened over the picture, so "
        "they only lighten it.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same streaks in the same places, another seed others.",
    )


@effect(category="atmosphere", params=RainParams)
def rain(rgb: np.ndarray, params: RainParams) -> np.ndarray:
    """Screen falling streaks of `color` over the picture, `density` of them per 1000 px², leaning by `angle`.

    Every streak lies at a random distance d from 0 (far) to 1 (near): its length and width are `length` and
    `thickness` times 1 - `depth` x (1 - d), and it screens that share of `color` along its middle, so the rain has
    depth. A streak is drawn with antialiased edges and is cut off by the picture's edge. Streaks are screened over
    the picture, so they only lighten it. A higher `density` keeps every streak of a lower one and adds more; the same
    `seed` always gives the same rain.
    """
    height, width = rgb.shape[:2]
    # constant: density is counted per 1000 square px
    streaks = np.random.default_rng(params.seed).random((round(params.density * height * width / 1000), 3))
    x, y, distance = streaks.T
    near = 1 - params.depth * (1 - distance)
    start = np.stack([x * width, y * height], axis=1)
    lean = np.radians(params.angle)
    end = start + params.length * near[:, None] * np.array([np.sin(lean), np.cos(lean)])
    layer = draw_segments((height, width), start, end, params.thickness * near / 2, near)
    light = layer[..., None] * np.asarray(params.color, np.float32)
    return screen(rgb, light)
