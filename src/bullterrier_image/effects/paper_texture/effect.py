"""The `paper_texture` effect: paper fibres, canvas or linen weave laid over the picture as a brightness texture."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.noise import fractal, standardise


class PaperTextureParams(EffectParams):
    """A surface texture multiplied into the picture: random fibres, optionally with a woven thread pattern."""

    kind: Literal["paper", "canvas", "linen"] = param(
        "paper",
        doc="The surface: 'paper' (random fibres only), 'canvas' (an even plain weave of threads `spacing` px apart, "
        "with the fibres mixed in) or 'linen' (the same weave, but every thread has its own random strength, so it "
        "looks uneven).",
    )
    amount: float = param(
        0.1,
        unit="fraction",
        min=0,
        max=1,
        doc="How strong the texture is: the standard deviation of the brightness change as a fraction of each pixel's "
        "own value (the texture multiplies the picture, so black stays black and the average brightness is kept). 0 = "
        "unchanged, 0.1 is visible, 0.3 is heavy; values lighten and darken alike, and the result is clipped to 0..1.",
    )
    scale: float = param(
        6.0,
        unit="px",
        min=1,
        max=256,
        doc="Size of the coarsest fibres in px, the spacing of the first noise lattice. Larger = broader, softer "
        "mottling; small values give a fine tooth.",
    )
    octaves: int = param(
        3,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed for the fibres, each finer than the last (by `lacunarity`) and "
        "weaker (by `persistence`). 1 = smooth blotches, more = more fine detail. Layers finer than one pixel are "
        "dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer of fibres is than the one before: each layer's weight is this to the "
        "power of its number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
        advanced=True,
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer of fibres is than the one before: the lattice spacing is divided by this per "
        "layer. 1 = every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
        advanced=True,
    )
    spacing: float = param(
        6.0,
        unit="px",
        min=3,
        max=64,
        doc="The thread pitch of 'canvas' and 'linen' in px: the distance from one thread to the next. Ignored for "
        "'paper'. Larger = coarser weave.",
    )
    fibre: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="For 'canvas' and 'linen', the share of the texture's variance that comes from the random fibres rather "
        "than the weave: 0 = a pure, regular weave, 1 = no weave (the same as 'paper'). Ignored for 'paper'.",
        advanced=True,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same texture (fibres and, for 'linen', thread strengths), "
        "another seed a different one.",
    )


@effect(category="texture", params=PaperTextureParams)
def paper_texture(rgb: np.ndarray, params: PaperTextureParams) -> np.ndarray:
    """Lay a paper, canvas or linen texture over the picture: each pixel is multiplied by 1 + `amount` x texture.

    The texture has zero mean and unit standard deviation over the picture, so `amount` is its relative strength and
    the average brightness is kept (before clipping to 0..1). The fibres are fractal noise (`scale`, `octaves`,
    `persistence`, `lacunarity`); a weave is a cosine wave of period `spacing` across the columns plus one down the
    rows, mixed with the fibres by `fibre`. The same texture multiplies every channel; the same `seed` gives the same
    texture.
    """
    height, width = rgb.shape[:2]
    texture = fractal(
        (height, width),
        scale=params.scale,
        octaves=params.octaves,
        persistence=params.persistence,
        lacunarity=params.lacunarity,
        seed=params.seed,
    )
    if params.kind != "paper":
        rng = np.random.default_rng(
            [params.seed, 1]
        )  # constant: stream number 1, apart from the fibres' lattices (stream 0)
        waves = []
        for size in (height, width):
            position = np.arange(size)
            wave = np.cos(2 * np.pi * position / params.spacing)
            if params.kind == "linen":  # every thread, one pitch wide, gets a strength of its own
                wave *= rng.random((size,))[(position / params.spacing).astype(int)]
            waves.append(wave)
        weave = standardise(waves[0][:, None] + waves[1][None, :])
        texture = standardise(np.sqrt(1 - params.fibre) * weave + np.sqrt(params.fibre) * texture)
    return np.clip(rgb * (1 + params.amount * texture[..., None]), 0.0, 1.0).astype(np.float32)
