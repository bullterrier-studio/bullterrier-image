"""The `rain` effect: falling streaks of light screened over the picture."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect


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
        doc="The length of the nearest streaks in px; the farthest are half as long. Tuned for a picture about 480 px "
        "high. Larger = longer streaks, as with a slower shutter.",
    )
    thickness: float = param(
        1.5,
        unit="px",
        min=1,
        max=20,
        doc="The width of the nearest streaks in px; the farthest are half as wide. Tuned for a picture about 480 px "
        "high. Larger = fatter drops.",
    )
    opacity: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright the nearest streaks are: the share of `color` they screen over the picture at their centre; "
        "the farthest are half as bright. 0 = invisible, 1 = solid colour.",
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


def _draw(
    shape: tuple[int, int], start: np.ndarray, end: np.ndarray, radius: np.ndarray, weight: np.ndarray
) -> np.ndarray:
    """The (h, w) float32 layer of the segments from `start` to `end` (n x 2 arrays of x, y), each `radius` px (n)
    to either side and as bright as its `weight` (n) at its centre line, with a one-pixel soft edge; where segments
    overlap the brighter one shows."""
    height, width = shape
    layer = np.zeros(shape, np.float32)
    for (x0, y0), (x1, y1), reach, light in zip(start, end, radius, weight, strict=True):
        top, bottom = max(int(min(y0, y1) - reach - 1), 0), min(int(max(y0, y1) + reach + 1) + 1, height)
        left, right = max(int(min(x0, x1) - reach - 1), 0), min(int(max(x0, x1) + reach + 1) + 1, width)
        rows, cols = np.mgrid[top:bottom, left:right]
        along = (x1 - x0, y1 - y0)
        # how far along the segment (0..1) the nearest point to each pixel is
        nearest = np.clip(((cols - x0) * along[0] + (rows - y0) * along[1]) / (along[0] ** 2 + along[1] ** 2), 0.0, 1.0)
        distance = np.hypot(cols - x0 - nearest * along[0], rows - y0 - nearest * along[1])
        # a pixel is a unit box: covered fully within `reach` - 0.5 of the segment, not at all beyond `reach` + 0.5
        cover = light * np.clip(reach + 0.5 - distance, 0.0, 1.0)
        layer[top:bottom, left:right] = np.maximum(layer[top:bottom, left:right], cover)
    return layer


@effect(category="atmosphere", params=RainParams)
def rain(rgb: np.ndarray, params: RainParams) -> np.ndarray:
    """Screen falling streaks of `color` over the picture, `density` of them per 1000 px², leaning by `angle`.

    Every streak has a random depth from 0 (far) to 1 (near): its length, width and brightness are `length`,
    `thickness` and `opacity` times 0.5 + 0.5 x depth, so the rain has depth. A streak is drawn with antialiased
    edges and is cut off by the picture's edge. Streaks are screened over the picture, so they only lighten it. A
    higher `density` keeps every streak of a lower one and adds more; the same `seed` always gives the same rain.
    """
    height, width = rgb.shape[:2]
    # constant: density is counted per 1000 square px
    streaks = np.random.default_rng(params.seed).random((round(params.density * height * width / 1000), 3))
    x, y, depth = streaks.T
    near = 0.5 + 0.5 * depth
    start = np.stack([x * width, y * height], axis=1)
    lean = np.radians(params.angle)
    end = start + params.length * near[:, None] * np.array([np.sin(lean), np.cos(lean)])
    layer = _draw((height, width), start, end, params.thickness * near / 2, params.opacity * near)
    # screen, 1 - (1 - rgb)(1 - light), written as rgb + light x (1 - rgb) so rgb is exactly unchanged where light is 0
    light = layer[..., None] * np.asarray(params.color, np.float32)
    return (rgb + light * (1 - rgb)).astype(np.float32)
