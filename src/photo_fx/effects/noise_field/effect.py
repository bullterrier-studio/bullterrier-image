"""The `noise_field` effect: a generated noise picture (value, perlin or worley) coloured through a ramp."""

import math
from typing import Literal

import numpy as np
from scipy.spatial import cKDTree
from scipy.special import ndtr

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal, standardise
from photo_fx.ops.ramp import ramp


class NoiseFieldParams(EffectParams):
    """A noise picture: `kind` of noise at `scale` px, layered by `octaves`, coloured through `colors`."""

    kind: Literal["value", "perlin", "worley"] = param(
        "perlin",
        doc="The noise: 'value' (random values on a lattice, smoothly interpolated: soft blotches), 'perlin' "
        "(gradient noise: random slopes on a lattice, finer and more even, zero at the lattice points) or 'worley' "
        "(cellular noise: the distance to the nearest of a scatter of random points, one per cell of `scale` px, "
        "dark at each point and brightening away from it; a single layer, so it ignores `octaves`, `persistence` "
        "and `lacunarity`).",
    )
    scale: float = param(
        120.0,
        unit="px",
        min=2,
        max=1024,
        doc="Size of the coarsest features in pixels: the spacing of the first lattice (value, perlin) or the size of "
        "a cell (worley); tuned for a picture about 480 px high. Larger = broad, slow variation; small = fine grain.",
    )
    octaves: int = param(
        4,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed (value, perlin), each finer than the last (by `lacunarity`) and "
        "weaker (by `persistence`). 1 = smooth, more = more fine detail. Layers finer than one pixel are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before (value, perlin): each layer's weight is this to "
        "the power of its number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before (value, perlin): the lattice spacing is divided by this "
        "per layer. 1 = every layer the same size, 2 = each half as coarse, larger = detail drops off faster.",
    )
    colors: tuple[Color, ...] = param(
        ("#0b1d3a", "#2a7f9e", "#f4e9c8"),
        doc="The colours the noise is shown in, from its lowest values to its highest, evenly spaced along the ramp; "
        "at least two. Each pixel's place on the ramp is its rank in the noise, so every colour is used about "
        "equally.",
        min_length=2,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same picture, another seed a different one.",
    )


def _corner(
    down: np.ndarray, across: np.ndarray, row: np.ndarray, col: np.ndarray, off_down: np.ndarray, off_across: np.ndarray
) -> np.ndarray:
    """A pixel's value from one lattice corner of its cell: the corner's random slope (`down`, `across` hold the slope
    of every lattice point) dotted with the pixel's offset from it. `row`, `col` are the corner's lattice indices
    for each pixel row and column, `off_down`, `off_across` the offsets from it in lattice units."""
    at = (row[:, None], col[None, :])
    return down[at] * off_down[:, None] + across[at] * off_across[None, :]


def _perlin(shape: tuple[int, int], params: NoiseFieldParams) -> np.ndarray:
    """Seeded gradient noise of `shape`, layered like `fractal`: random slopes at the lattice points, blended."""
    rng = np.random.default_rng(params.seed)
    height, width = shape
    field = np.zeros(shape, np.float32)
    for octave in range(params.octaves):
        spacing = params.scale / params.lacunarity**octave
        if spacing < 1:
            break
        y, x = np.arange(height) / spacing, np.arange(width) / spacing  # places in lattice units
        row, col = y.astype(int), x.astype(int)
        fy, fx = y - row, x - col  # where in its cell each row and column lies, 0 to 1
        lattice = (row[-1] + 2, col[-1] + 2)
        slope_down, slope_across = rng.standard_normal(lattice), rng.standard_normal(lattice)
        top_left = _corner(slope_down, slope_across, row, col, fy, fx)
        top_right = _corner(slope_down, slope_across, row, col + 1, fy, fx - 1)
        bottom_left = _corner(slope_down, slope_across, row + 1, col, fy - 1, fx)
        bottom_right = _corner(slope_down, slope_across, row + 1, col + 1, fy - 1, fx - 1)
        # constant: Perlin's improved-noise fade curve 6t^5 - 15t^4 + 10t^3 (SIGGRAPH 2002), zero slope at both ends
        fade_y, fade_x = (t * t * t * (t * (t * 6 - 15) + 10) for t in (fy, fx))
        top = top_left + (top_right - top_left) * fade_x[None, :]
        bottom = bottom_left + (bottom_right - bottom_left) * fade_x[None, :]
        field += params.persistence**octave * (top + (bottom - top) * fade_y[:, None])
    return standardise(field)


def _worley(shape: tuple[int, int], params: NoiseFieldParams) -> np.ndarray:
    """Distance in px from every pixel to the nearest of a jittered grid of feature points, one per `scale` px cell."""
    height, width = shape
    rows, cols = math.ceil(height / params.scale) + 2, math.ceil(width / params.scale) + 2  # a ring of cells around
    rng = np.random.default_rng(params.seed)
    cell_y, cell_x = np.indices((rows, cols)) - 1
    point_y = (cell_y + rng.random((rows, cols))) * params.scale
    point_x = (cell_x + rng.random((rows, cols))) * params.scale
    pixel_y, pixel_x = np.indices(shape)
    distance, _ = cKDTree(np.column_stack((point_y.ravel(), point_x.ravel()))).query(
        np.column_stack((pixel_y.ravel(), pixel_x.ravel()))
    )
    return standardise(distance.reshape(shape))


@effect(category="backgrounds", params=NoiseFieldParams)
def noise_field(rgb: np.ndarray, params: NoiseFieldParams) -> np.ndarray:
    """Replace the picture with a noise field of `kind` (value, perlin or worley), coloured through `colors`.

    A generated background: the input is used only for its height and width, its content is ignored. The noise is
    standardised to zero mean and unit deviation over the picture and passed through the normal cumulative
    distribution, so a pixel's place on the colour ramp is about its rank among all pixels. 'value' is `fractal`
    noise; 'perlin' sums octaves of gradient noise (a Gaussian random slope at each lattice point, blended with
    Perlin's fade curve); 'worley' is the distance to the nearest feature point (F1).
    """
    shape = rgb.shape[:2]
    if params.kind == "value":
        noise = fractal(
            shape,
            scale=params.scale,
            octaves=params.octaves,
            persistence=params.persistence,
            lacunarity=params.lacunarity,
            seed=params.seed,
        )
    elif params.kind == "perlin":
        noise = _perlin(shape, params)
    else:
        noise = _worley(shape, params)
    return ramp(ndtr(noise).astype(np.float32), params.colors)
