"""Seeded procedural noise fields for effects that need texture, flow or randomness that looks organic.

Every field is a float32 array of shape `(height, width)` with values in [0, 1], and is a pure function of its
arguments: the same arguments always give the same array, on any machine, and the global numpy random state is never
touched. A field is defined by pixel coordinates, not by the canvas: `fractal_noise` of a big shape, cropped to the top
left corner, equals `fractal_noise` of the small shape, so the same seed and scale give the same texture at any
output size. (`worley` sizes its cells relative to the canvas and so does not promise this.)
"""

import math
import numbers
import operator
from typing import Literal

import numpy as np
from scipy.spatial import cKDTree

# Most octaves `fractal_noise` accepts. Halving 32 times takes a feature from 4 billion pixels to one, more than any
# picture holds, and a bound is needed because with `lacunarity=1` no octave ever becomes too fine to drop.
_MAX_OCTAVES = 32

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


def _check_shape(shape: tuple[int, int]) -> tuple[int, int]:
    try:
        height, width = (operator.index(n) for n in shape)
    except (TypeError, ValueError):
        raise ValueError(f"shape must be (height, width), two integers; got {shape!r}") from None
    if height < 1 or width < 1:
        raise ValueError(f"shape must have a positive height and width; got {shape!r}")
    return height, width


def _check_number(name: str, value: float) -> None:
    # bool is a number to Python but never what a caller means by a size or a count.
    if isinstance(value, bool) or not isinstance(value, numbers.Real):
        raise ValueError(f"{name} must be a number; got {value!r}")


def _check_positive(name: str, value: float) -> None:
    _check_number(name, value)
    if not (math.isfinite(value) and value > 0):
        raise ValueError(f"{name} must be a positive, finite number; got {value!r}")


def _check_choice(name: str, value: str, allowed: tuple[str, ...]) -> None:
    if value not in allowed:
        raise ValueError(f"{name} must be one of {', '.join(allowed)}; got {value!r}")


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
    lacunarity: float = 2.0,
    kind: Literal["gradient", "value"] = "gradient",
) -> np.ndarray:
    """Smooth noise, optionally with finer detail layered on top.

    The field depends only on pixel coordinates, `scale`, `seed` and the octave settings, never on `shape`: cropping
    the top left corner of a larger field gives exactly the smaller field.

    Args:
        shape: `(height, width)` of the result in pixels, both positive.
        scale: size in pixels of the largest feature (the lattice spacing of the first octave), positive and finite.
            Below 1 it acts as 1.
        seed: any integer (taken modulo 2**64); each seed gives an unrelated field.
        octaves: number of layers, an integer from 1 to 32. Each extra octave has features `lacunarity` times
            smaller than the previous one (half the size by default) and `persistence` times its weight. Octaves
            whose features would be under one pixel are left out (they only add aliasing), so very fine scales may
            use fewer octaves than asked.
        persistence: weight of each octave relative to the previous one, positive and finite; below 1 the fine
            detail is fainter than the coarse shape, above 1 it dominates. Any positive finite value is
            accepted, however extreme; the result is always finite and within [0, 1] (only the ratios matter).
        lacunarity: factor by which the feature size shrinks from one octave to the next, at least 1 and finite.
            2 (the default) halves it; larger values spread the octaves further apart in size, 1 stacks
            independent layers of the same size. Only matters when `octaves` is above 1.
        kind: "gradient" for Perlin-style noise (zero-mean ripples, no visible grid), "value" for interpolated
            random lattice values (blobbier, with plateaus).

    Returns:
        float32 array of `shape` with values in [0, 1].

    The curve that blends lattice points is fixed (Perlin's quintic, 6t^5 - 15t^4 + 10t^3, which has no creases:
    its first and second derivatives are continuous), as are the sampling at pixel centres and the normalisation
    that maps the field into [0, 1]; none of them is a style choice.
    """
    height, width = _check_shape(shape)
    _check_positive("scale", scale)
    _check_positive("persistence", persistence)
    _check_number("lacunarity", lacunarity)
    if not (math.isfinite(lacunarity) and lacunarity >= 1):
        raise ValueError(f"lacunarity must be a finite number of at least 1; got {lacunarity!r}")
    _check_choice("kind", kind, ("gradient", "value"))
    if isinstance(octaves, bool) or not isinstance(octaves, numbers.Integral):
        raise ValueError(f"octaves must be an integer; got {octaves!r}")
    if not 1 <= octaves <= _MAX_OCTAVES:
        raise ValueError(f"octaves must be between 1 and {_MAX_OCTAVES}; got {octaves!r}")

    spacings = [scale]
    while len(spacings) < octaves:
        spacings.append(spacings[-1] / lacunarity)  # step by step: lacunarity ** octave can overflow where this cannot
        if spacings[-1] < 1.0:
            spacings.pop()  # this octave and every finer one would only add aliasing
            break

    # Weights are persistence ** octave, scaled so the largest is exactly 1 (the first octave when persistence <= 1,
    # the last one above 1): only the ratios matter, and this keeps every power in the float range. The tiny
    # weights that underflow to 0 then belong to octaves that contribute nothing anyway.
    exponents = range(len(spacings)) if persistence <= 1 else range(1 - len(spacings), 1)
    weights = [persistence**exponent for exponent in exponents]

    total = np.zeros((height, width), dtype=np.float32)
    for octave, (spacing, weight) in enumerate(zip(spacings, weights, strict=True)):
        # A lattice finer than the pixel grid is just white noise, and a table of it would be bigger than the image:
        # so the first octave's features are never smaller than one pixel either (later octaves that fine were
        # dropped above, so only the first can need the clamp).
        total += np.float32(weight) * _octave((height, width), max(spacing, 1.0), seed, octave, kind)
    weight_sum = sum(weights)

    if kind == "gradient":
        total = total * np.float32(0.5 / (_GRADIENT_BOUND * weight_sum)) + np.float32(0.5)
    else:
        total = total * np.float32(1.0 / weight_sum)
    return np.clip(total, 0.0, 1.0, out=total)


def worley(
    shape: tuple[int, int],
    cells: float,
    seed: int,
    *,
    metric: Literal["f1", "f2-f1"] = "f1",
) -> np.ndarray:
    """Cellular noise: distance to the nearest of a scatter of random feature points.

    The canvas is divided into square cells, each holding one feature point at a random position, so the points
    are evenly spread but irregular (a point lies anywhere in its cell with equal chance; that is fixed, not a style
    choice, and the same goes for the Euclidean distance). Unlike `fractal_noise`, the cell size is relative to the
    canvas (the longer side holds `cells` cells), so the same seed on a different shape gives different cells, not a
    crop.

    Args:
        shape: `(height, width)` of the result in pixels, both positive.
        cells: approximate number of cells across the longer side, positive and finite (below 1 means a cell larger
            than the canvas; more than the longer side in pixels acts as one cell per pixel). A square canvas holds
            about `cells` squared feature points.
        seed: any integer (taken modulo 2**64); each seed gives a different scatter of points.
        metric: "f1" is the distance to the nearest feature point: 0 at every point, brightening away from it
            (cells look like round blobs). "f2-f1" is the distance to the second nearest minus that to the nearest:
            0 along the borders between cells, bright at their centres (cells look like tiles with dark seams).

    Returns:
        float32 array of `shape` with values in [0, 1]. Distances are measured in cell widths, so 1.0 is one cell
        and the few larger values (about 1 pixel in 1000) are clipped. The scale does not depend on the data.
    """
    height, width = _check_shape(shape)
    _check_positive("cells", cells)
    _check_choice("metric", metric, ("f1", "f2-f1"))
    # More cells than pixels would only build a table far bigger than the image; a cell is never under one pixel.
    cell = max(1.0, max(height, width) / cells)
    rows, columns = math.ceil(height / cell), math.ceil(width / cell)

    # One point per grid cell, for a ring of two extra cells around the canvas so that points just outside it
    # still count as neighbours of the pixels near the edge.
    margin = 2
    hashes = _lattice_hash(rows + 2 * margin, columns + 2 * margin, _key(seed, 0))
    jitter_y = _unit_interval(hashes)
    jitter_x = _unit_interval(_mix(hashes + np.uint64(_GOLDEN)))
    grid_y, grid_x = np.mgrid[-margin : rows + margin, -margin : columns + margin]
    points = np.stack([(grid_y + jitter_y).ravel(), (grid_x + jitter_x).ravel()], axis=1) * cell

    # Exact nearest neighbours (a fixed 3 x 3 neighbourhood scan can miss the true second nearest point).
    pixel_y, pixel_x = np.mgrid[0:height, 0:width]
    pixels = np.stack([pixel_y.ravel() + 0.5, pixel_x.ravel() + 0.5], axis=1)
    distance = cKDTree(points).query(pixels, k=2, workers=-1)[0] / cell

    field = distance[:, 0] if metric == "f1" else distance[:, 1] - distance[:, 0]
    return np.clip(field, 0.0, 1.0).astype(np.float32).reshape(height, width)
