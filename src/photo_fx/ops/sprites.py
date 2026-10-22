"""Many small soft sprites over a picture: seeded placement and vectorised drawing.

For effects that scatter lots of little things (rain, snow, sparkles, dust, embers, lens drops, digital rain,
stippling). `scatter` decides where the sprites are, how big, how near and with which animation phase, from a seed
alone; `stamp` draws them onto an image and returns a new one. The two are separate so an animated effect can move the
same sprites (`x`, `y`, `phase`) from frame to frame and re-stamp them.

Conventions: pixel `(row, col)` covers `[col, col+1) x [row, row+1)`, so a sprite at `(x, y)` is centred in that pixel
when both are `col + 0.5` and `row + 0.5`; "the picture" is `0 <= x < width`, `0 <= y < height`. Images are float32
RGB `(h, w, 3)` in 0..1. Nothing here depends on the effects package.
"""

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class Placement:
    """Where and how big a set of sprites is. All fields are float32 arrays of the same length (one entry per sprite).

    x, y: centre in pixels (x to the right, y downward; see the module note on pixel centres).
    size: diameter in pixels.
    depth: 0..1, 0 = far, 1 = near. Nearer sprites are larger and brighter.
    phase: 0..1, a per-sprite random number for animation offsets (when it blinks, how far it has fallen).
    """

    x: np.ndarray
    y: np.ndarray
    size: np.ndarray
    depth: np.ndarray
    phase: np.ndarray


def scatter(
    shape: tuple[int, int], count: int, seed: int, *, size: tuple[float, float] = (2.0, 8.0), depth_layers: int = 3
) -> Placement:
    """Place `count` sprites uniformly over a picture, in a few discrete depth layers.

    shape: (height, width) of the picture in pixels, both >= 1.
    count: number of sprites, >= 0 (0 gives empty arrays).
    seed: seeds `numpy.random.default_rng`; the same arguments always give the same placement.
    size: (smallest, largest) diameter in pixels, 0 < smallest <= largest. The farthest layer gets the smallest size and
        the nearest the largest; all sprites in a layer share one size.
    depth_layers: number of distinct depths, >= 1. With L > 1 the depths are evenly spaced from 0 to 1 and, when
        `count >= L`, every layer is used (layer populations differ by at most one sprite). One layer means every
        sprite is at depth 1 (near) and has the largest size; pass `size=(s, s)` for a particular fixed size.
    """
    if len(shape) != 2 or min(shape) < 1:
        raise ValueError(f"shape must be (height, width) with both >= 1, got {shape}")
    if count < 0:
        raise ValueError(f"count must be >= 0, got {count}")
    lo, hi = size
    if not 0 < lo <= hi:
        raise ValueError(f"size must be a range 0 < smallest <= largest, got {size}")
    if depth_layers < 1:
        raise ValueError(f"depth_layers must be >= 1, got {depth_layers}")

    h, w = shape
    rng = np.random.default_rng(seed)
    x = rng.random(count) * w
    y = rng.random(count) * h
    layer = rng.permutation(count) % depth_layers
    phase = rng.random(count)

    layer_depth = np.linspace(0.0, 1.0, depth_layers) if depth_layers > 1 else np.ones(1)
    depth = layer_depth[layer]

    def f32(a: np.ndarray, upper: float | None = None) -> np.ndarray:
        out = a.astype(np.float32)
        # rounding to float32 can lift a value just below `upper` up to it; keep the half-open range
        return out if upper is None else np.minimum(out, np.nextafter(np.float32(upper), np.float32(0)))

    return Placement(f32(x, w), f32(y, h), f32(lo + (hi - lo) * depth), f32(depth), f32(phase, 1.0))
