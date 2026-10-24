"""The `torn_edge` effect: the picture's border torn away along a ragged line and replaced by a paper colour."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.noise import fractal

TOP, BOTTOM, LEFT, RIGHT = 0, 1, 2, 3  # constant: the four sides, numbered so each tears from a seed of its own


class TornEdgeParams(EffectParams):
    """A torn paper edge: along each side everything beyond a wandering tear line is replaced by the paper colour."""

    depth: float = param(
        24.0,
        unit="px",
        min=0,
        max=200,
        doc="How far in the tear is from each side on average, in px: 0 = no tear. Larger = more of the border torn "
        "away.",
    )
    roughness: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How ragged the tear is: the standard deviation of its distance from the side as a fraction of `depth`. "
        "0 = a straight cut exactly `depth` in, 0.3 is a gentle tear, 1 wanders between the very edge and twice the "
        "depth.",
    )
    scale: float = param(
        24.0,
        unit="px",
        min=1,
        max=200,
        doc="Size of the tear's broadest bays and tongues along the side, in px (the spacing of the first noise "
        "lattice). Larger = long slow waves, small = a jagged edge.",
    )
    octaves: int = param(
        3,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of detail make up the tear, each finer than the last (by `lacunarity`) and weaker (by "
        "`persistence`). 1 = smooth waves, more = a fibrous, ragged edge. Layers finer than a pixel are dropped.",
    )
    persistence: float = param(
        0.4,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer of the tear is than the one before. 0 = only the broadest layer, 1 = "
        "every layer equally strong (very jagged).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer of the tear is than the one before: the lattice spacing is divided by this per "
        "layer. 1 = every layer the same size, larger = detail much finer than the waves.",
    )
    fill: Color = param(
        "#f5f0e6",
        doc="The paper colour that replaces the torn-away border: '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always tears the same line (each side its own), another seed a different one.",
    )


@effect(category="texture", params=TornEdgeParams)
def torn_edge(rgb: np.ndarray, params: TornEdgeParams) -> np.ndarray:
    """Tear the border away: each side's pixels closer than a wandering distance of about `depth` px become `fill`.

    The distance along a side is `depth` x (1 + `roughness` x noise), the noise fractal (`scale`, `octaves`,
    `persistence`, `lacunarity`) of zero mean and unit deviation along that side, so it averages `depth` and has a
    standard deviation `roughness` x `depth` (a negative distance tears nothing there). A pixel is torn away if it is
    closer to a side than that side's distance at its position. Every other pixel is kept exactly; the four sides tear
    independently, and the same `seed` always gives the same tear.
    """
    height, width = rgb.shape[:2]

    def distance(length: int, side: int) -> np.ndarray:
        """How far the tear is from `side` (`TOP`, `BOTTOM`, `LEFT`, `RIGHT`) at each of `length` positions along it."""
        # constant: a strip one pixel high; the two low bits of a side's seed number the side, so no two share a tear
        wander = fractal(
            (1, length),
            scale=params.scale,
            octaves=params.octaves,
            persistence=params.persistence,
            lacunarity=params.lacunarity,
            seed=(params.seed << 2) | side,
        )
        return params.depth * (1 + params.roughness * wander[0])

    rows, cols = np.arange(height)[:, None], np.arange(width)[None, :]
    torn = (
        (rows < distance(width, TOP)[None, :])
        | (height - 1 - rows < distance(width, BOTTOM)[None, :])
        | (cols < distance(height, LEFT)[:, None])
        | (width - 1 - cols < distance(height, RIGHT)[:, None])
    )
    return np.where(torn[..., None], np.asarray(params.fill, np.float32), rgb).astype(np.float32)
