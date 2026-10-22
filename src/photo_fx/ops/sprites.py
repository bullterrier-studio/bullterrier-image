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
from typing import Literal

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


# Memory bounds only; they never change the result (a test compares batched and one-by-one drawing).
_MAX_PATCH_ELEMENTS = 2_000_000  # pixels evaluated at once for a batch of sprites
_MAX_PENDING_ENTRIES = 4_000_000  # drawn pixels held before equal pixels are summed together
# Numerical guard, not a look parameter: keeps log(1 - a) finite for a fully opaque sprite. It moves the result by
# less than 1e-7, below the resolution of the float32 output.
_ALMOST_ONE = 1.0 - 1e-7
_PLACEMENT_FIELDS = ("x", "y", "size", "depth", "phase")


# Each blend turns the sprite colour into a pair: "terms" (what one sprite pixel of alpha `a` contributes) and
# "resolve" (the canvas plus the SUM of the terms of all sprites over a pixel, to the final pixel). Summing makes the
# outcome independent of sprite order, and for one colour it is exactly what drawing the sprites one after the other
# would give (screen: factors 1 - a*c multiply; over: factors 1 - a multiply), so a whole batch of sprites can be
# accumulated without a loop over sprites.
def _add(color: np.ndarray):
    def terms(alpha):
        return alpha[:, None] * color

    def resolve(canvas, acc):
        return canvas + acc

    return terms, resolve


def _screen(color: np.ndarray):
    def terms(alpha):
        return -np.log1p(-np.minimum(alpha[:, None] * color, _ALMOST_ONE))

    def resolve(canvas, acc):
        return 1.0 - (1.0 - canvas) * np.exp(-acc)

    return terms, resolve


def _over(color: np.ndarray):
    def terms(alpha):
        return -np.log1p(-np.minimum(alpha, _ALMOST_ONE))[:, None]

    def resolve(canvas, acc):
        keep = np.exp(-acc)
        return canvas * keep + color * (1.0 - keep)

    return terms, resolve


_BLENDS = {"add": _add, "screen": _screen, "over": _over}


def _unit_interval(value: float, name: str) -> None:
    if not 0.0 <= value <= 1.0:  # also false for NaN
        raise ValueError(f"{name} must be in 0..1, got {value}")


def _checked(canvas, placement, color, opacity, shape, angle, length, blend, feather, softness, far, near):
    """Validate `stamp`'s arguments; return the sprite fields x, y, size, depth and the colour as float64 arrays."""
    canvas = np.asarray(canvas)
    if canvas.ndim != 3 or canvas.shape[2] != 3:
        raise ValueError(f"canvas must have shape (height, width, 3), got {canvas.shape}")
    if blend not in _BLENDS:
        raise ValueError(f"blend must be one of {sorted(_BLENDS)}, got {blend!r}")
    if shape not in ("disc", "streak"):
        raise ValueError(f"shape must be 'disc' or 'streak', got {shape!r}")
    _unit_interval(opacity, "opacity")
    _unit_interval(far, "far_brightness")
    _unit_interval(near, "near_brightness")
    color_arr = np.asarray(color, dtype=np.float64)
    if color_arr.shape != (3,) or not np.isfinite(color_arr).all():
        raise ValueError(f"color must be three finite numbers (r, g, b), got {color}")
    if not length >= 1.0:
        raise ValueError(f"length must be >= 1, got {length}")
    if not np.isfinite(angle):
        raise ValueError(f"angle must be finite, got {angle}")
    if not 0.0 < feather < np.inf:
        raise ValueError(f"feather must be a positive, finite number of pixels, got {feather}")
    if not 0.0 <= softness < np.inf:
        raise ValueError(f"softness must be a non-negative, finite fraction of the radius, got {softness}")
    fields = [np.asarray(getattr(placement, name), dtype=np.float64) for name in _PLACEMENT_FIELDS]
    if any(f.ndim != 1 or f.shape != fields[0].shape for f in fields):
        raise ValueError("placement fields must be 1-D arrays of the same length")
    x, y, size, depth, _ = fields
    if not all(np.isfinite(f).all() for f in (x, y, size, depth)):
        raise ValueError("placement has NaN or infinite values")
    return x, y, size, depth, color_arr


def _sum_equal_pixels(pixels: list[np.ndarray], terms: list[np.ndarray]) -> tuple[np.ndarray, np.ndarray]:
    """Sum the terms of entries that fall on the same pixel; returns sorted unique pixel indices and their sums."""
    flat, t = np.concatenate(pixels), np.concatenate(terms)
    order = np.argsort(flat, kind="stable")
    flat, t = flat[order], t[order]
    first = np.flatnonzero(np.r_[True, flat[1:] != flat[:-1]])
    return flat[first], np.add.reduceat(t, first, axis=0)


def stamp(
    canvas: np.ndarray,
    placement: Placement,
    *,
    color: tuple[float, float, float] = (1.0, 1.0, 1.0),
    opacity: float = 1.0,
    shape: Literal["disc", "streak"] = "disc",
    angle: float = 0.0,
    length: float = 4.0,
    blend: Literal["add", "screen", "over"] = "screen",
    feather: float = 1.0,
    softness: float = 0.5,
    far_brightness: float = 0.4,
    near_brightness: float = 1.0,
) -> np.ndarray:
    """Draw soft sprites on a picture and return the result as a new array; `canvas` is not modified.

    canvas: float32 RGB `(h, w, 3)`, values 0..1 (float64 is accepted and converted).
    placement: the sprites, from `scatter` or built by hand. Positions are in pixels (see the module note); sprites
        partly or wholly outside the picture are clipped, never wrapped around, and never an error.
    color: RGB of every sprite, each 0..1 (more than 1 is allowed and only meaningful for `blend="add"`).
    opacity: 0..1, strength of the nearest sprites (those at depth 1, scaled by `near_brightness`).
    shape: "disc", an antialiased soft disc of diameter `placement.size`; or "streak", the same disc stretched
        `length` times along `angle`, for rain. A streak's `size` is its width, so it is `size * length` long.
    angle: degrees, direction of a streak: 0 points right, 90 points down the picture. Ignored for discs.
    length: >= 1, how many times longer than wide a streak is. Ignored for discs.
    blend: "add" adds light (and clips at 1 afterwards), "screen" lightens without ever exceeding 1,
        "over" paints `color` over the picture. Overlapping sprites combine without regard to their order.
    feather: > 0, pixels. The narrowest allowed width of the sprite's soft edge, centred on its rim. 1 is just enough
        to antialias; raising it blurs small sprites (a sprite no wider than the feather becomes a dim blob).
    softness: >= 0, fraction of the radius. The edge of larger sprites is `softness * radius` wide (never less than
        `feather`). 0.5 gives a soft rim around a flat core; near 0 gives crisp discs; above 1 no core is left.
    far_brightness: 0..1, brightness of a sprite at depth 0 (far). Multiplies `opacity`. Raise it to 1 so that
        distance does not dim sprites; with `blend="over"` that is also what lets far sprites fully cover the picture.
    near_brightness: 0..1, brightness at depth 1 (near). Brightness varies linearly with depth in between.

    Light follows area: a sprite twice as large adds about four times the light, within about 6% from a diameter of
    2 px upward. Below that the edge softening dominates and sprites come out brighter than their area: about 1.3x
    at 1 px and 3.8x at 0.5 px. Pixels no sprite touches are returned unchanged to the last bit; touched pixels
    are clipped to 0..1. Memory grows with the area drawn, not with the size of the picture beyond the returned copy;
    cost grows with sprite area, so long streaks of large sprites (area x `length`) are the slow case.
    """
    x, y, size, depth, color_arr = _checked(
        canvas,
        placement,
        color,
        opacity,
        shape,
        angle,
        length,
        blend,
        feather,
        softness,
        far_brightness,
        near_brightness,
    )
    out = np.array(canvas, dtype=np.float32, copy=True)
    if len(x) == 0:
        return out

    h, w = out.shape[:2]
    terms, resolve = _BLENDS[blend](color_arr)

    radius = size / 2.0
    soft = np.maximum(feather, softness * radius)
    reach = radius + soft / 2.0  # beyond this distance from its centre a sprite is invisible
    stretch, direction = (length, np.radians(angle)) if shape == "streak" else (1.0, 0.0)
    cos, sin = np.cos(direction), np.sin(direction)
    # how far the stretched sprite reaches along x and y, as a multiple of `reach`
    spread_x, spread_y = np.hypot(stretch * cos, sin), np.hypot(stretch * sin, cos)
    brightness = far_brightness + (near_brightness - far_brightness) * np.clip(depth, 0.0, 1.0)
    gain = opacity * brightness

    # Drop sprites that cannot touch the picture while still in float, so absurd coordinates never reach integer code.
    ex, ey = reach * spread_x, reach * spread_y
    visible = (x + ex > 0) & (x - ex < w) & (y + ey > 0) & (y - ey < h) & (gain > 0) & (size > 0)
    if not visible.any():
        return out

    # Drawn pixels are collected as (pixel index, terms) entries; entries on the same pixel are summed from time to
    # time, so memory follows the area drawn instead of the size of the picture.
    entry_pixels, entry_terms, entries = [], [], 0
    # Sprites are batched by the whole-pixel width of their patch, so a patch is a small dense array and one numpy
    # expression evaluates the profile of every sprite in the batch.
    # (capped: a patch never needs more than the picture's size, and a huge float must not overflow the integer)
    patch_class = np.ceil(np.minimum(reach, 1e9)).astype(np.int64)
    for cls in np.unique(patch_class[visible]):
        ids = np.flatnonzero(visible & (patch_class == cls))
        kx = int(min(np.ceil(cls * spread_x) + 1, w - 1))
        ky = int(min(np.ceil(cls * spread_y) + 1, h - 1))
        offsets_x, offsets_y = np.arange(-kx, kx + 1), np.arange(-ky, ky + 1)
        per_chunk = max(1, _MAX_PATCH_ELEMENTS // (offsets_x.size * offsets_y.size))
        for start in range(0, len(ids), per_chunk):
            sel = ids[start : start + per_chunk]
            # The window is centred on the sprite's centre moved into the picture: for a sprite centred far outside
            # (or larger than the picture) that still covers every pixel it can reach.
            cx = np.clip(np.floor(x[sel]), 0, w - 1).astype(np.int64)
            cy = np.clip(np.floor(y[sel]), 0, h - 1).astype(np.int64)
            cols, rows = cx[:, None] + offsets_x, cy[:, None] + offsets_y
            # (+ 0.5: a pixel's centre is half a pixel in from its corner, the module's coordinate convention)
            dx = (cols + 0.5 - x[sel, None])[:, None, :]
            dy = (rows + 0.5 - y[sel, None])[:, :, None]
            # distance in the sprite's own frame, where a streak is a disc: along its axis lengths shrink by `stretch`
            dist = np.hypot((dx * cos + dy * sin) / stretch, -dx * sin + dy * cos)
            r, s = radius[sel, None, None], soft[sel, None, None]
            alpha = np.clip((r + s / 2.0 - dist) / s, 0.0, 1.0) * gain[sel, None, None]
            inside = ((cols >= 0) & (cols < w))[:, None, :] & ((rows >= 0) & (rows < h))[:, :, None] & (alpha > 0)
            entry_pixels.append((rows[:, :, None] * w + cols[:, None, :])[inside])
            entry_terms.append(terms(alpha[inside]))
            entries += len(entry_pixels[-1])
            if entries >= _MAX_PENDING_ENTRIES:
                merged_pixels, merged_terms = _sum_equal_pixels(entry_pixels, entry_terms)
                entry_pixels, entry_terms, entries = [merged_pixels], [merged_terms], len(merged_pixels)

    if entries == 0:  # every sprite was transparent where it overlaps the picture
        return out
    touched, acc = _sum_equal_pixels(entry_pixels, entry_terms)
    drawn = acc.any(axis=1)  # a black sprite adds nothing under `add`: leave those pixels exactly as they were
    touched, acc = touched[drawn], acc[drawn]
    pixels = out.reshape(h * w, 3)
    pixels[touched] = np.clip(resolve(pixels[touched].astype(np.float64), acc), 0.0, 1.0)
    return out
