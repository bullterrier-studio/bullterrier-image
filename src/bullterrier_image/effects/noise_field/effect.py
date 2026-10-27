"""The `noise_field` effect: a generated noise picture (value, perlin, worley or sine) coloured through a ramp."""

import math
from collections.abc import Sequence
from typing import Literal

import numpy as np
from scipy.spatial import cKDTree
from scipy.special import ndtr

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.noise import fractal, standardise
from bullterrier_image.ops.ramp import ramp
from bullterrier_image.ops.warp import remap


class NoiseFieldParams(EffectParams):
    """A noise picture: `kind` of noise at `scale` px, layered by `octaves`, pushed about by `warp`, coloured through
    `colors`."""

    kind: Literal["value", "perlin", "worley", "sine"] = param(
        "perlin",
        doc="The noise: 'value' (random values on a lattice, smoothly interpolated: soft blotches), 'perlin' "
        "(gradient noise: random slopes on a lattice, finer and more even, zero at the lattice points) or 'worley' "
        "(cellular noise: the distance to the nearest of a scatter of random points, one per cell of `scale` px, "
        "dark at each point and brightening away from it; each layer is a finer scatter) or 'sine' (crossed sine "
        "waves: smooth, regular swirls; the classic plasma).",
    )
    scale: float = param(
        120.0,
        unit="px",
        min=2,
        max=1024,
        doc="Size of the coarsest features in pixels: the spacing of the first lattice (value, perlin), the size of "
        "a cell (worley) or the wavelength from crest to crest of the first waves (sine); tuned for a picture about "
        "480 px high. Larger = broad, slow variation; small = fine grain.",
    )
    octaves: int = param(
        4,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed (all kinds), each finer than the last (by `lacunarity`) and "
        "weaker (by `persistence`). 1 = smooth, more = more fine detail. Each sine layer is two waves crossing at "
        "random angles. Layers finer than one pixel (sine: two pixels) are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before (all kinds): each layer's weight is "
        "this to the power of its number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before (all kinds): the lattice spacing, cell size or "
        "wavelength is divided by this per layer. 1 = every layer the same size, 2 = each half as coarse, larger = "
        "detail drops off faster.",
    )
    warp: float = param(
        0.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels the pattern is pushed about, the typical shift of a point: the swirling, folded look "
        "of a domain warp. 0 = no warp, and a little is nearly the same picture. In 'sine' the first layer is never "
        "pushed, so with one octave `warp` changes nothing.",
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


def _perlin(shape: tuple[int, int], params: NoiseFieldParams, seed: int | Sequence[int]) -> np.ndarray:
    """Seeded gradient noise of `shape`, layered like `fractal`: random slopes at the lattice points, blended."""
    rng = np.random.default_rng(seed)
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


def _worley(shape: tuple[int, int], params: NoiseFieldParams, seed: int | Sequence[int]) -> np.ndarray:
    """Seeded fractal F1 of `shape`, layered like `fractal`: per layer the distance to the nearest point of a jittered
    grid, in cells (so every layer has the same range whatever its cell size)."""
    height, width = shape
    rng = np.random.default_rng(seed)
    pixels = np.column_stack([axis.ravel() for axis in np.indices(shape)])
    field = np.zeros(shape, np.float32)
    for octave in range(params.octaves):
        cell = params.scale / params.lacunarity**octave
        if cell < 1:
            break
        rows, cols = math.ceil(height / cell) + 2, math.ceil(width / cell) + 2  # a ring of cells around
        cell_y, cell_x = np.indices((rows, cols)) - 1
        point_y = (cell_y + rng.random((rows, cols))) * cell
        point_x = (cell_x + rng.random((rows, cols))) * cell
        distance, _ = cKDTree(np.column_stack((point_y.ravel(), point_x.ravel()))).query(pixels)
        field += params.persistence**octave * (distance / cell).reshape(shape)
    return standardise(field)


def _wave(
    rng: np.random.Generator,
    x: np.ndarray,
    y: np.ndarray,
    wavelength: float,
    push_x: np.ndarray | float,
    push_y: np.ndarray | float,
) -> np.ndarray:
    """One plane sine wave of `wavelength` px in a random direction and phase, read at the positions x and y (a row and
    a column of pixel places) pushed along by `push_x` and `push_y` px."""
    angle, phase = math.tau * rng.random(), math.tau * rng.random()
    return np.sin(math.tau / wavelength * ((x + push_x) * math.cos(angle) + (y + push_y) * math.sin(angle)) + phase)


def _sine(shape: tuple[int, int], params: NoiseFieldParams) -> np.ndarray:
    """Seeded sine waves of `shape`: per layer two plane waves, each layer pushed about by the one before."""
    height, width = shape
    rng = np.random.default_rng(params.seed)
    y, x = np.arange(height, dtype=np.float32)[:, None], np.arange(width, dtype=np.float32)[None, :]
    total = np.zeros(shape, np.float32)
    push_x: np.ndarray = np.zeros_like(x)
    push_y: np.ndarray = np.zeros_like(y)
    for octave in range(params.octaves):
        wavelength = params.scale / params.lacunarity**octave
        if wavelength < 2:  # constant: a wave needs at least 2 px a period to show on a pixel grid (Nyquist)
            break
        first = _wave(rng, x, y, wavelength, push_x, push_y)
        second = _wave(rng, x, y, wavelength, push_x, push_y)
        total += params.persistence**octave * (first + second)
        push_x, push_y = params.warp * first, params.warp * second
    return standardise(total)


def _noise(shape: tuple[int, int], params: NoiseFieldParams, seed: int | Sequence[int]) -> np.ndarray:
    """The standardised noise of `params.kind` and `seed` over `shape`, before any warp of the whole picture."""
    if params.kind == "value":
        return fractal(
            shape,
            scale=params.scale,
            octaves=params.octaves,
            persistence=params.persistence,
            lacunarity=params.lacunarity,
            seed=seed,
        )
    if params.kind == "perlin":
        return _perlin(shape, params, seed)
    if params.kind == "worley":
        return _worley(shape, params, seed)
    return _sine(shape, params)


@effect(category="backgrounds", params=NoiseFieldParams)
def noise_field(rgb: np.ndarray, params: NoiseFieldParams) -> np.ndarray:
    """Replace the picture with a noise field of `kind` (value, perlin, worley, the Voronoi-like one, or sine),
    coloured through `colors`.

    A generated background: the input is used only for its height and width, its content is ignored. Followed by
    `shatter` with `fill` 1 it makes flat Voronoi cells (see the `voronoi-cells` recipe). The noise is
    standardised to zero mean and unit deviation over the picture and passed through the normal cumulative
    distribution, so a pixel's place on the colour ramp is about its rank among all pixels. 'value' is `fractal`
    noise; 'perlin' sums octaves of gradient noise (a Gaussian random slope at each lattice point, blended with
    Perlin's fade curve); 'worley' sums octaves of F1 (the distance to the nearest feature point, one point per cell
    of `scale` / `lacunarity`**i, measured in those cells), weight `persistence`**i; 'sine' sums octaves of two plane
    sine waves sin(2 pi (x cos a + y sin a) / w + p) of wavelength w = `scale` / `lacunarity`**i, random
    directions a and phases p, weight `persistence`**i, layer i evaluated at positions pushed along x and y by `warp`
    px times the two waves of layer i - 1.

    For the other kinds `warp` reads the finished picture at points shifted by two further fields of the kind (the
    seed's streams 1 and 2, standardised, so their deviation is `warp` px), bicubically, mirrored beyond the edge.
    """
    shape = rgb.shape[:2]
    place = ndtr(_noise(shape, params, params.seed)).astype(np.float32)
    if params.kind != "sine" and params.warp > 0:
        # constant: streams 1 and 2 of the seed are the independent fields that push rows and columns; 0 is the picture
        push_y, push_x = (params.warp * _noise(shape, params, [params.seed, stream]) for stream in (1, 2))
        rows, cols = np.indices(shape, dtype=np.float32)
        place = remap(place[..., None], rows + push_y, cols + push_x, interpolation="bicubic", edge="mirror")[..., 0]
    return ramp(place, params.colors)
