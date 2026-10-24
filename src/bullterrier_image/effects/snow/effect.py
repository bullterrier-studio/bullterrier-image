"""The `snow` effect: soft white flakes in depth layers, big and bright near, small and faint far."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.light import screen
from bullterrier_image.ops.particles import draw_segments


class SnowParams(EffectParams):
    """Snow: round white flakes in several depth layers, the near ones large, soft and bright."""

    density: float = param(
        0.5,
        unit="count",
        min=0,
        max=5,
        doc="How many flakes fall: flakes per 1000 square px of picture (a 32 x 32 px area), so the same density "
        "looks alike on any picture size. 0 = none (the picture unchanged), 0.5 is a light flurry, 3 a blizzard.",
    )
    size: float = param(
        14.0,
        unit="px",
        min=1,
        max=100,
        doc="The diameter of the nearest, largest flakes in px; flakes in farther layers are smaller. Tuned for a "
        "picture about 480 px high. Larger = bigger flakes, as if they fell closer to the lens.",
    )
    layers: int = param(
        4,
        unit="count",
        min=1,
        max=10,
        doc="How many depth layers the flakes fall in. The flake in layer k of n is k / n as wide as `size` and "
        "(1 - `depth` x (1 - k / n)) as bright as solid white, so more layers = a smoother range from fine far flakes "
        "to large near ones. 1 = every flake alike.",
    )
    softness: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How soft a flake's edge is: the edge falls from full brightness to none across this fraction of the "
        "flake's radius (at least one pixel), so large flakes are blurrier than small ones, like flakes out of "
        "focus. 0 = crisp one-pixel edges, 1 = a flake fading out from its middle.",
    )
    depth: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much fainter the farthest flakes are: a flake in layer k of n has 1 - depth x (1 - k / n) of the "
        "brightness of the nearest. 0 = every layer equally bright, 1 = the far layers fade to nothing.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same flakes in the same places, another seed others.",
    )


@effect(category="atmosphere", params=SnowParams, opacity=0.8)
def snow(rgb: np.ndarray, params: SnowParams) -> np.ndarray:
    """Screen white flakes over the picture, `density` of them per 1000 px², in `layers` depth layers.

    Each flake falls in a random layer; at nearness d = k / `layers` (the nearest layer is 1) it is a disc `size` x d
    wide, 1 - `depth` x (1 - d) bright (1 = solid white), with an edge `softness` of its radius soft. Where flakes
    overlap the brighter one shows, and flakes are screened over the picture, so they only lighten it. A higher
    `density` keeps every flake of a lower one and adds more; the same `seed` always gives the same snow.
    """
    height, width = rgb.shape[:2]
    # constant: density is counted per 1000 square px
    flakes = np.random.default_rng(params.seed).random((round(params.density * height * width / 1000), 3))
    x, y, layer = flakes.T
    near = (np.floor(layer * params.layers) + 1) / params.layers
    centre = np.stack([x * width, y * height], axis=1)
    radius = params.size / 2 * near
    light = draw_segments(
        (height, width),
        centre,
        centre,
        radius,
        1 - params.depth * (1 - near),
        params.softness * radius,
    )
    return screen(rgb, light[..., None])
