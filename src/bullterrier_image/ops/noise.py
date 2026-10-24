"""Seeded noise fields: fractal value noise (in pixels, or in shares of the frame), and the normalisation they share."""

import math
from collections.abc import Sequence
from typing import Literal

import numpy as np
from scipy.interpolate import RectBivariateSpline

from bullterrier_image.ops.blur import gaussian_blur


def standardise(field: np.ndarray) -> np.ndarray:
    """`field` shifted to zero mean and scaled to unit standard deviation, as float32.

    A field with no variation (a single value, a constant) has nothing to scale: it becomes all zeros.
    """
    centred = field - field.mean()
    spread = centred.std()
    return (centred / (spread if spread > 0 else 1.0)).astype(np.float32)


def fractal(
    shape: tuple[int, int],
    *,
    scale: float,
    octaves: int,
    persistence: float,
    lacunarity: float,
    seed: int | Sequence[int],
) -> np.ndarray:
    """Seeded fractal value noise of `shape` (height, width): float32, zero mean and unit standard deviation.

    The field is a pure function of its arguments (same arguments, same array, on any machine); `seed` is an int, or
    a list of ints such as [seed, n] for the n-th independent field of one seed. Each octave i is a
    lattice of independent standard-normal values spaced `scale / lacunarity**i` px apart, interpolated up to the
    picture by an interpolating bicubic spline (so the field passes through its lattice values and is smooth between
    them); the octaves are summed with weights `persistence**i`, so octave 0 is the coarsest and, with persistence 0,
    the only one. An octave whose spacing is under 1 px is finer than the pixel grid and is left out, together with
    every later one. The sum is then standardised over the whole picture.
    """
    height, width = shape
    rng = np.random.default_rng(seed)
    field = np.zeros(shape)
    for octave in range(octaves):
        spacing = scale / lacunarity**octave
        if spacing < 1:
            break
        # lattice points to reach the last pixel
        # constant: a cubic spline needs at least four points along each axis
        rows, cols = (max(math.ceil((size - 1) / spacing) + 1, 4) for size in shape)
        lattice = rng.standard_normal((rows, cols))
        spline = RectBivariateSpline(np.arange(rows), np.arange(cols), lattice)
        field += persistence**octave * spline(np.arange(height) / spacing, np.arange(width) / spacing)
    return standardise(field)


def frame_fractal(
    shape: tuple[int, int],
    *,
    scale: float,
    octaves: int,
    persistence: float,
    lacunarity: float,
    seed: int | Sequence[int],
) -> np.ndarray:
    """Seeded fractal value noise of `shape` (height, width) laid on the frame, so that the same field comes out at
    any resolution: float32, zero mean and unit standard deviation.

    Like `fractal`, but `scale` is a share of the picture's long side and the lattice is fixed in the frame: octave i
    has independent standard-normal values spaced `scale / lacunarity**i` of the long side apart, covering the square
    that holds the picture whatever its size, and each pixel samples the interpolating bicubic spline through them at
    its centre. Two sizes of one picture share the lattice and differ only by sampling, and by the octaves finer than
    a pixel, which a small picture leaves out (with every later one). A crop of the long side's square (shape
    `(h, w)` against `(w, w)`) is the same field, apart from the final standardisation over the whole picture.
    """
    long_side = max(shape)
    rng = np.random.default_rng(seed)
    field = np.zeros(shape)
    for octave in range(octaves):
        spacing = scale / lacunarity**octave
        if spacing * long_side < 1:
            break
        # lattice points to reach the far side of the square
        # constant: a cubic spline needs at least four points along each axis
        points = max(math.ceil(1 / spacing) + 1, 4)
        lattice = rng.standard_normal((points, points))
        spline = RectBivariateSpline(np.arange(points), np.arange(points), lattice)
        rows, cols = ((np.arange(size) + 0.5) / long_side / spacing for size in shape)  # pixel centres
        field += persistence**octave * spline(rows, cols)
    return standardise(field)


def clumped_noise(
    shape: tuple[int, int],
    *,
    size: float,
    seed: int,
    truncate: float,
    edge: Literal["reflect", "nearest", "mirror", "wrap"],
) -> np.ndarray:
    """Seeded white noise of `shape` (height, width), float32, clumped into specks about `size` px wide.

    Up to a `size` of 1 every pixel is an independent standard-normal value. Above 1 the noise is blurred by a Gaussian
    of sigma `size / 2` (cut off at `truncate` sigmas, `edge` deciding what lies beyond the border, as in
    `ops.blur.gaussian_blur`) and scaled back to unit standard deviation, so its strength does not depend on `size`.
    A pure function of its arguments.
    """
    noise = np.random.default_rng(seed).standard_normal(shape).astype(np.float32)
    if size > 1.0:  # a speck of 1 px or less is a single pixel: nothing to clump
        noise = gaussian_blur(noise, size / 2.0, truncate=truncate, edge=edge)
        noise /= max(float(noise.std()), 1e-6)  # constant: guard against dividing by zero on a 1-pixel picture
    return noise
