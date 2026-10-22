"""Seeded procedural noise fields for effects that need texture, flow or randomness that looks organic.

Every field is a float32 array of shape `(height, width)` with values in [0, 1], and is a pure function of its
arguments: the same arguments always give the same array, on any machine, and the global numpy random state is never
touched. A field is defined by pixel coordinates, not by the canvas: `fractal_noise` of a big shape, cropped to the top
left corner, equals `fractal_noise` of the small shape, so the same seed and scale give the same texture at any
output size. (`worley` sizes its cells relative to the canvas and so does not promise this.)
"""

from typing import Literal

import numpy as np

_GOLDEN = 0x9E3779B97F4A7C15
_MASK_64 = 0xFFFFFFFFFFFFFFFF
_COLUMN_PRIME = np.uint64(0xC2B2AE3D27D4EB4F)
_ROW_PRIME = np.uint64(0x165667B19E3779F9)
_TWO_POW_MINUS_53 = 2.0**-53

# Unit gradient directions for Perlin-style noise: 256 evenly spaced angles, picked by a hash byte. A table (rather than
# cos/sin of a hashed angle at every lattice point) keeps results bit-identical whatever the size of the array computed.
_ANGLES = np.arange(256) * (2.0 * np.pi / 256.0)
_GRADIENT_X = np.cos(_ANGLES).astype(np.float32)
_GRADIENT_Y = np.sin(_ANGLES).astype(np.float32)

# Largest possible value of 2-D gradient noise built from unit gradients: sqrt(2) / 2. Dividing by this fixed bound
# (not by the data's own extremes) keeps a field independent of how much of it is visible.
_GRADIENT_BOUND = 0.5 * np.sqrt(2.0)


def _mix(z: np.ndarray) -> np.ndarray:
    """Scramble uint64 values (splitmix64 finaliser): near inputs give unrelated outputs."""
    z = z ^ (z >> np.uint64(30))
    z = z * np.uint64(0xBF58476D1CE4E5B9)
    z = z ^ (z >> np.uint64(27))
    z = z * np.uint64(0x94D049BB133111EB)
    return z ^ (z >> np.uint64(31))


def _key(seed: int, *stream: int) -> np.uint64:
    """A 64-bit hash key for `seed` and a sub-stream (octave number, purpose), so streams never share randomness."""
    z = np.array([seed & _MASK_64], dtype=np.uint64)
    for part in stream:
        z = _mix(z + np.uint64((_GOLDEN * (part + 1)) & _MASK_64))
    return z[0]


def _lattice_hash(rows: int, columns: int, key: np.uint64) -> np.ndarray:
    """A (rows, columns) array of uint64 hashes, one per integer lattice point, fixed by the point and the key."""
    row = np.arange(rows, dtype=np.uint64)[:, None] * _ROW_PRIME
    column = np.arange(columns, dtype=np.uint64)[None, :] * _COLUMN_PRIME
    return _mix(row + column + key)


def _unit_interval(hashes: np.ndarray) -> np.ndarray:
    """Map uint64 hashes to float64 in [0, 1)."""
    return (hashes >> np.uint64(11)).astype(np.float64) * _TWO_POW_MINUS_53


def _fade(t: np.ndarray) -> np.ndarray:
    """Perlin's quintic ease: 0 -> 0, 1 -> 1, flat at both ends, so neighbouring lattice cells blend without creases."""
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)


def _octave(shape: tuple[int, int], spacing: float, seed: int, octave: int, kind: str) -> np.ndarray:
    """One octave of lattice noise with lattice points `spacing` pixels apart.

    Returns float32 in [0, 1] for "value" and in [-_GRADIENT_BOUND, _GRADIENT_BOUND] for "gradient".
    """
    height, width = shape
    # Sample at pixel centres, shifted by a seeded fraction of a cell, so no lattice point sits exactly on pixel (0, 0)
    # and the lattices of different octaves do not share their (zero-valued, for gradient noise) points.
    shift = _unit_interval(_lattice_hash(1, 2, _key(seed, octave, 1)))[0]
    ux = (np.arange(width) + 0.5) / spacing + shift[0]
    uy = (np.arange(height) + 0.5) / spacing + shift[1]
    ix, iy = np.floor(ux).astype(np.intp), np.floor(uy).astype(np.intp)
    tx, ty = (ux - ix).astype(np.float32), (uy - iy).astype(np.float32)
    sx, sy = _fade(tx), _fade(ty)

    hashes = _lattice_hash(iy[-1] + 2, ix[-1] + 2, _key(seed, octave, 0))
    if kind == "value":
        values = _unit_interval(hashes).astype(np.float32)
        along_x = values[:, ix] * (1 - sx) + values[:, ix + 1] * sx  # blend each lattice row along x first
        return along_x[iy] * (1 - sy)[:, None] + along_x[iy + 1] * sy[:, None]

    direction = (hashes >> np.uint64(56)).astype(np.intp)
    gx, gy = _GRADIENT_X[direction], _GRADIENT_Y[direction]
    # Each corner contributes gradient . (offset to the corner). Blending the x part and the y part of that dot
    # product separately along x first leaves only a cheap per-row combination at full image size.
    x_part = gx[:, ix] * tx * (1 - sx) + gx[:, ix + 1] * (tx - 1) * sx
    y_slope = gy[:, ix] * (1 - sx) + gy[:, ix + 1] * sx
    top = x_part[iy] + y_slope[iy] * ty[:, None]
    bottom = x_part[iy + 1] + y_slope[iy + 1] * (ty - 1)[:, None]
    return top * (1 - sy)[:, None] + bottom * sy[:, None]


def fractal_noise(
    shape: tuple[int, int],
    scale: float,
    seed: int,
    *,
    octaves: int = 1,
    persistence: float = 0.5,
    kind: Literal["gradient", "value"] = "gradient",
) -> np.ndarray:
    """Smooth noise, optionally with finer detail layered on top.

    The field depends only on pixel coordinates, `scale`, `seed` and the octave settings, never on `shape`: cropping
    the top left corner of a larger field gives exactly the smaller field.

    Args:
        shape: `(height, width)` of the result in pixels, both positive.
        scale: size in pixels of the largest feature (the lattice spacing of the first octave), a positive number.
        seed: any integer; each seed gives an unrelated field.
        octaves: number of layers, at least 1. Each extra octave has features half the size of the previous one
            and `persistence` times its weight. Octaves whose features would be under one pixel are left out
            (they only add aliasing), so very fine scales may use fewer octaves than asked.
        persistence: weight of each octave relative to the previous one, positive; below 1 the fine detail is
            fainter than the coarse shape.
        kind: "gradient" for Perlin-style noise (zero-mean ripples, no visible grid), "value" for interpolated
            random lattice values (blobbier, with plateaus).

    Returns:
        float32 array of `shape` with values in [0, 1].
    """
    height, width = shape
    total = np.zeros((height, width), dtype=np.float32)
    weight_sum = 0.0
    for octave in range(octaves):
        spacing = scale / 2**octave
        if octave > 0 and spacing < 1.0:
            break
        weight = persistence**octave
        total += np.float32(weight) * _octave(shape, spacing, seed, octave, kind)
        weight_sum += weight

    if kind == "gradient":
        total = total * np.float32(0.5 / (_GRADIENT_BOUND * weight_sum)) + np.float32(0.5)
    else:
        total = total * np.float32(1.0 / weight_sum)
    return np.clip(total, 0.0, 1.0, out=total)
