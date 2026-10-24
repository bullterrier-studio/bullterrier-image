"""Seeded noise fields: fractal value noise, and the normalisation noise fields share."""

import math

import numpy as np
from scipy.interpolate import RectBivariateSpline


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
    seed: int,
) -> np.ndarray:
    """Seeded fractal value noise of `shape` (height, width): float32, zero mean and unit standard deviation.

    The field is a pure function of its arguments (same arguments, same array, on any machine). Each octave i is a
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
