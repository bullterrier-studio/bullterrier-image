"""A partition drawn so that its pieces can be told apart and counted: the picture dimmed, a white line along every
border and a white number in each piece, for a person who has to say which piece a recipe means."""

import numpy as np
from scipy import ndimage

from .glyphs import glyph_atlas

_DIM = 0.35  # constant: the picture dimmed to this share, so white lines and numbers stand out on any photo
_DIGIT_SHARE = 1 / 32  # constant: a digit is this share of the long side wide ...
_DIGIT_LEAST = 6  # constant: ... and at least this many px, the smallest a digit can be read at


def border_distance(labels: np.ndarray) -> np.ndarray:
    """How far each pixel's centre is from the nearest border between two pieces of `labels` (int32 (h, w)), in px,
    float64 (h, w): a pixel beside a border is half a px from it, and the distance grows by the straight-line distance
    to such pixels (to about a pixel's accuracy along a slanting border). `inf` everywhere for a single piece."""
    beside = np.zeros(labels.shape, bool)
    apart = labels[:, 1:] != labels[:, :-1]
    beside[:, 1:] |= apart
    beside[:, :-1] |= apart
    above = labels[1:] != labels[:-1]
    beside[1:] |= above
    beside[:-1] |= above
    if not beside.any():
        return np.full(labels.shape, np.inf)
    return ndimage.distance_transform_edt(~beside) + 0.5  # constant: the pixels beside a border are half a px away


def number_pieces(rgb: np.ndarray, labels: np.ndarray, count: int) -> np.ndarray:
    """`rgb` (float32 (h, w, 3)) dimmed, with a white line on each side of every border between the pieces of `labels`
    (int32 (h, w), the pieces numbered 0..`count`-1) and, in each piece, its number in white, centred on the pixel
    of the piece nearest its middle (a number wider than the picture is cut off at its edge). A new array."""
    height, width = labels.shape
    out = np.maximum(rgb * np.float32(_DIM), (border_distance(labels) < 1)[..., None])  # the lines, white
    cell = max(round(max(labels.shape) * _DIGIT_SHARE), _DIGIT_LEAST)
    digits = {digit: glyph_atlas(digit, cell)[0] for digit in "0123456789"}
    flat = labels.ravel()
    order = np.argsort(flat)
    starts = np.searchsorted(flat[order], np.arange(count + 1))
    for piece in range(count):
        y, x = np.divmod(order[starts[piece] : starts[piece + 1]], width)
        middle = np.argmin((y - y.mean()) ** 2 + (x - x.mean()) ** 2)
        ink = np.concatenate([digits[digit] for digit in str(piece)], axis=1)
        top, left = int(y[middle]) - ink.shape[0] // 2, int(x[middle]) - ink.shape[1] // 2
        rows = slice(max(top, 0), min(top + ink.shape[0], height))
        columns = slice(max(left, 0), min(left + ink.shape[1], width))
        shown = ink[rows.start - top : rows.stop - top, columns.start - left : columns.stop - left, None]
        out[rows, columns] = out[rows, columns] * (1 - shown) + shown
    return out
