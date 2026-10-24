"""Moving a picture, or one of its channels, by a distance in pixels: whole or fractional, with a rule for the edge."""

import math
from typing import Literal

import numpy as np

Edge = Literal["wrap", "nearest", "reflect", "mirror"]
EDGES = ("wrap", "nearest", "reflect", "mirror")


def shift(image: np.ndarray, dy: float, dx: float, *, edge: Edge) -> np.ndarray:
    """Move `image` (height, width, ...) down by `dy` and right by `dx` pixels; a negative distance moves it up or left.

    The output has the shape and dtype of the input, is always a new array, and reads the input only, so the input is
    never modified. A distance that is not a whole number of pixels is read between the two lines it lies between,
    linearly, along each axis (so both together are bilinear): a pixel moved by 1.25 puts three quarters of its value
    on the pixel 1 away and a quarter on the pixel 2 away. The picture's total is kept with `edge` 'wrap'.

    `edge` says what lies beyond the picture's edge, which a shift uncovers on one side: 'wrap' the picture repeated
    (what leaves comes back on the other side), 'nearest' its outermost line repeated, 'reflect' the picture
    mirrored with the edge line repeated once (`dcba|abcd|dcba`), 'mirror' mirrored about the edge line
    (`dcb|abcd|cba`). Any distance is allowed, also beyond the picture's size. Anything else for `edge` is a
    `ValueError`.
    """
    if edge not in EDGES:
        raise ValueError(f"unknown edge {edge!r}: use one of {', '.join(EDGES)}")
    out = image
    for axis, distance in enumerate((dy, dx)):
        if distance != 0:
            out = _shift_axis(out, distance, axis, edge)
    return out.copy() if out is image else out


def _shift_axis(image: np.ndarray, distance: float, axis: int, edge: Edge) -> np.ndarray:
    """`image` moved by `distance` px along one axis: output line i is the input at i - distance."""
    size = image.shape[axis]
    whole = math.floor(distance)
    fraction = distance - whole
    lines = np.arange(size)
    # the output at line i lies between the input lines i - whole - 1 and i - whole
    near = np.take(image, _line_within(lines - whole, size, edge), axis=axis)
    if fraction == 0:
        return near
    far = np.take(image, _line_within(lines - whole - 1, size, edge), axis=axis)
    return (near * (1 - fraction) + far * fraction).astype(image.dtype)


def _line_within(index: np.ndarray, size: int, edge: Edge) -> np.ndarray:
    """The line of a picture of `size` lines that each position of `index` shows, by the rule for beyond its edge."""
    if edge == "wrap":
        return index % size
    if edge == "nearest":
        return np.clip(index, 0, size - 1)
    period = 2 * size if edge == "reflect" else max(2 * size - 2, 1)
    folded = index % period
    return np.where(folded >= size, period - (folded + 1 if edge == "reflect" else folded), folded)
