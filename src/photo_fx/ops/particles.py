"""Drawing particles: soft discs and streaks, each at its own place, size and brightness, onto one layer of light."""

import numpy as np


def draw_segments(
    shape: tuple[int, int],
    start: np.ndarray,
    end: np.ndarray,
    radius: float | np.ndarray,
    weight: float | np.ndarray,
    feather: float | np.ndarray = 1.0,  # constant: one pixel, the finest edge a pixel grid can draw
) -> np.ndarray:
    """The (height, width) float32 layer of light that segments from `start` to `end` leave, brightest where they cross.

    `start` and `end` are (n, 2) arrays of x, y positions in px, a pixel's centre at whole numbers; every `start`
    lies inside the picture, an `end` may lie beyond it. A segment whose ends are equal is a disc. `radius` (the
    half-width of the segment, px), `weight` (its brightness along its centre line, 0..1) and `feather` (the width of
    its soft edge, px) are each one number for every segment or an array of n. A feather under 1 is taken as 1: a
    narrower edge is aliasing, the pixel grid's own limit; without one it is 1, a pixel of antialiasing.

    A pixel's value is `weight` times how far the segment covers it: 1 within `radius` - `feather` / 2 of the segment
    (its centre line, or the point of a disc), 0 beyond `radius` + `feather` / 2, and falling linearly between, so
    exactly 0.5 at `radius`; a feather of 1 is one pixel of antialiasing. Where segments overlap the brighter one
    shows: light does not add up. Parts beyond the picture's edge are cut off.
    """
    height, width = shape
    layer = np.zeros(shape, np.float32)
    count = len(start)
    radii, weights, feathers = (np.broadcast_to(value, count) for value in (radius, weight, feather))
    feathers = np.maximum(feathers, 1.0)
    for (x0, y0), (x1, y1), reach, light, edge in zip(start, end, radii, weights, feathers, strict=True):
        margin = reach + edge / 2 + 1
        top, bottom = max(int(min(y0, y1) - margin), 0), min(int(max(y0, y1) + margin) + 1, height)
        left, right = max(int(min(x0, x1) - margin), 0), min(int(max(x0, x1) + margin) + 1, width)
        rows, cols = np.mgrid[top:bottom, left:right]
        along = (x1 - x0, y1 - y0)
        squared = along[0] ** 2 + along[1] ** 2
        # how far along the segment (0..1) the nearest point to each pixel is
        # constant: a point has no length to divide by, and its nearest point is its own
        nearest = np.clip(((cols - x0) * along[0] + (rows - y0) * along[1]) / squared, 0.0, 1.0) if squared else 0.0
        distance = np.hypot(cols - x0 - nearest * along[0], rows - y0 - nearest * along[1])
        cover = light * np.clip((reach + 0.5 * edge - distance) / edge, 0.0, 1.0)
        layer[top:bottom, left:right] = np.maximum(layer[top:bottom, left:right], cover)
    return layer
