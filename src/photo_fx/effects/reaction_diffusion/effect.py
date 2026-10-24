"""The `reaction_diffusion` effect: a Gray-Scott pattern (spots, worms, mazes) grown from the picture's bright parts."""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.ramp import ramp
from photo_fx.ops.warp import remap

# constant: the diffusion rates of chemicals u and v of the Gray-Scott model in grid units per step, a ratio of 2 to 1
# (Pearson 1993, "Complex patterns in a simple system": 2.097e-5 and 1.05e-5 on a grid spacing of 0.01, time step 1)
_DIFFUSION_U, _DIFFUSION_V = 0.2097, 0.105
# constant: where the picture seeds the pattern, chemical u falls to half and v rises to a quarter (Pearson 1993)
_SEED_U, _SEED_V = 0.5, 0.25


class ReactionDiffusionParams(EffectParams):
    """A Gray-Scott reaction-diffusion simulation, seeded by the picture and drawn through a colour ramp."""

    feed: float = param(
        0.037,
        unit="ratio",
        min=0.01,
        max=0.08,
        doc="The feed rate F of the Gray-Scott model: how fast chemical u is replenished. Together with `kill` it "
        "decides the kind of pattern: about 0.03 to 0.04 with a kill of 0.06 gives spots and worms, higher feeds "
        "give mazes and fingerprints, the extremes leave nothing but the seed.",
    )
    kill: float = param(
        0.06,
        unit="ratio",
        min=0.04,
        max=0.07,
        doc="The kill rate k of the Gray-Scott model: how fast chemical v is removed. Raising it makes the pattern "
        "sparser (isolated spots); lowering it fills the picture.",
    )
    steps: int = param(
        1000,
        unit="count",
        min=0,
        max=5000,
        doc="How many simulation steps run. 0 shows only the seed; more lets the pattern grow outward from it and "
        "costs time in proportion (and more cells, so smaller `cell`, cost more per step).",
    )
    cell: float = param(
        4.0,
        unit="px",
        min=1,
        max=64,
        doc="The size of one simulation cell in px: the model runs on a coarse grid and is read back over the picture "
        "by `resampling`. 1 simulates every pixel (slow on large pictures); larger = a coarser, larger-scale "
        "pattern that runs faster. Tuned for a picture about 480 px high.",
    )
    threshold: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=1,
        doc="Brightness (0..1) above which the picture seeds the pattern: only the brighter parts start it. Lower "
        "seeds more of the picture; at 1 nothing seeds and the result is a flat first colour.",
    )
    colors: tuple[Color, ...] = param(
        ("#12002b", "#6a1bff", "#ff2bd6", "#ffd84a"),
        doc="The colour ramp, evenly spaced; at least two. Untouched areas take the first colour and the strongest "
        "pattern the last.",
        min_length=2,
    )
    resampling: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How the picture is read at each cell and how the cells are read back over the picture: 'nearest' takes "
        "the closest cell (blocky), 'bilinear' blends the four around (smooth), 'bicubic' fits a cubic spline (the "
        "smoothest, may overshoot slightly).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What lies beyond the picture's border for the diffusion and for reading: 'reflect' (mirrored, edge "
        "pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge, so the pattern tiles).",
    )


@effect(category="stylised", params=ReactionDiffusionParams)
def reaction_diffusion(rgb: np.ndarray, params: ReactionDiffusionParams) -> np.ndarray:
    """Grow a Gray-Scott reaction-diffusion pattern (spots, worms, mazes) from the picture's bright parts.

    The model has two chemicals, u (starting at 1 everywhere) and v (starting at 0). Where the picture's brightness
    is above `threshold` they start at u = 1/2, v = 1/4. Each step both diffuse (v half as fast as u, by the
    five-point Laplacian) and react: u += Du lap u - u v^2 + feed (1 - u), v += Dv lap v + u v^2 - (feed + kill) v,
    with Du = 0.2097 and Dv = 0.105 (Pearson 1993) and a time step of 1. The result is drawn through `colors` at
    1 - u, so untouched areas take the first colour. The simulation runs on cells of `cell` px: the picture is read
    at each cell's centre, and the cells are read back over the picture, both by `resampling`.
    """
    height, width = rgb.shape[:2]
    rows, cols = -(-height // params.cell), -(-width // params.cell)  # cells down and across, rounded up
    # cell centres in picture pixel indices (a pixel's centre is at whole numbers)
    seen_from = np.broadcast_arrays(
        ((np.arange(rows) + 0.5) * params.cell - 0.5)[:, None], ((np.arange(cols) + 0.5) * params.cell - 0.5)[None, :]
    )
    brightness = remap(luminance(rgb)[..., None], *seen_from, interpolation=params.resampling, edge=params.edge)
    seeded = brightness[..., 0] > params.threshold
    u = np.where(seeded, _SEED_U, 1.0).astype(np.float32)
    v = np.where(seeded, _SEED_V, 0.0).astype(np.float32)
    for _ in range(params.steps):
        reaction = u * v * v
        u, v = (
            u + _DIFFUSION_U * ndimage.laplace(u, mode=params.edge) - reaction + params.feed * (1 - u),
            v + _DIFFUSION_V * ndimage.laplace(v, mode=params.edge) + reaction - (params.feed + params.kill) * v,
        )
    # read the cells back over the picture: pixel centres in cell indices
    back = np.broadcast_arrays(
        ((np.arange(height) + 0.5) / params.cell - 0.5)[:, None],
        ((np.arange(width) + 0.5) / params.cell - 0.5)[None, :],
    )
    dropped = remap((1 - u)[..., None], *back, interpolation=params.resampling, edge=params.edge)[..., 0]
    return ramp(dropped, params.colors)
