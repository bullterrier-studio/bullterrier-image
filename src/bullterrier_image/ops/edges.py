"""Edges: how fast a picture's brightness changes at each pixel."""

import numpy as np
from scipy import ndimage

from bullterrier_image.ops.blur import NUMPY_PAD, gaussian_blur
from bullterrier_image.ops.convolve import Edge


def sobel_gradient(grey: np.ndarray, *, edge: Edge) -> tuple[np.ndarray, np.ndarray]:
    """The Sobel gradient of `grey` (height, width): how fast the brightness rises across (left to right) and down (top
    to bottom) at each pixel, in brightness per px. Both results have the input's shape.

    A plane that rises 0.1 per px across reads (0.1, 0) wherever the picture does not end; `edge` is what lies beyond
    the picture, as in `bullterrier_image.ops.convolve` ('reflect', 'mirror', 'nearest' or 'wrap'), and decides the
    first and last row and column only. `grey` is never modified.
    """
    padded = np.pad(grey, 1, mode=NUMPY_PAD[edge])
    across_x = padded[:, 2:] - padded[:, :-2]  # right neighbour minus left, for every padded row
    across_y = padded[2:, :] - padded[:-2, :]  # lower neighbour minus upper, for every padded column
    # The Sobel kernels (Sobel and Feldman): the difference across, smoothed 1-2-1 along the edge.
    gx = across_x[:-2] + 2 * across_x[1:-1] + across_x[2:]
    gy = across_y[:, :-2] + 2 * across_y[:, 1:-1] + across_y[:, 2:]
    # constant: a slope of 1 per px reads 2 across the difference and 4 from the 1-2-1 weights, so Sobel is 8 times it
    return gx / 8, gy / 8


def edge_strength(grey: np.ndarray, *, sigma: float, truncate: float, edge: Edge) -> np.ndarray:
    """How steeply the brightness of `grey` (height, width) changes at each pixel, in brightness per px: the Sobel
    gradient magnitude of the picture smoothed by a Gaussian of `sigma` px (0 = not smoothed), `truncate` sigmas wide.

    The result has the input's shape and is 0 where the picture is flat. Inside a ramp that rises 0.1 per px it is 0.1,
    whichever way the ramp runs; a step of 0.6 between two pixels reads 0.3 on each of them, and smoothing spreads it
    over more pixels at a lower peak. `edge` is what lies beyond the picture ('reflect', 'mirror', 'nearest' or 'wrap');
    it decides the strength in the first and last row and column only (and what the smoothing sees there). `grey` is
    never modified.
    """
    smooth = gaussian_blur(grey, sigma, truncate=truncate, edge=edge)
    return np.hypot(*sobel_gradient(smooth, edge=edge))


# Four of the eight neighbours as (down, across) offsets, clockwise from "right" in picture terms; the other four are
# their opposites.
_AROUND = ((0, 1), (1, 1), (1, 0), (1, -1))  # constant: the neighbours of a pixel, as offsets


def ridges(strength: np.ndarray, gx: np.ndarray, gy: np.ndarray) -> np.ndarray:
    """Where `strength` (height, width) is at its crest across the edge, as a boolean array: the thin lines of Canny's
    non-maximum suppression.

    `gx`, `gy` are the gradient `strength` is the length of (from `sobel_gradient`). A pixel stays if no neighbour on
    either side of it, along the gradient's direction (rounded to the nearest of eight), is stronger. Of two equal
    neighbours across a line one stays, so a step between two pixels leaves a line one pixel wide. Beyond the picture
    there is no strength, so a border pixel can stay.
    """
    # constant: the eight directions are pi/4 apart (a full turn is 2 pi)
    direction = np.rint(np.arctan2(gy, gx) / (np.pi / 4)).astype(np.int64)
    padded = np.pad(strength, 1)
    height, width = strength.shape
    keep = np.zeros(strength.shape, bool)
    for axis, (down, across) in enumerate(_AROUND):
        after = padded[1 + down : 1 + down + height, 1 + across : 1 + across + width]
        before = padded[1 - down : 1 - down + height, 1 - across : 1 - across + width]
        along = (direction % 4) == axis  # constant: opposite directions are 4 apart (of 8) and give the same line
        keep |= along & (strength >= after) & (strength > before)
    return keep


def connected_to(candidates: np.ndarray, seeds: np.ndarray) -> np.ndarray:
    """The pixels of `candidates` (boolean, height by width) that touch, directly or through other candidates, a pixel
    of `seeds`: Canny's hysteresis. Touching includes the diagonal. Every seed is a candidate.
    """
    labels, _ = ndimage.label(candidates, structure=np.ones((3, 3), bool))  # constant: 3x3 = diagonals touch
    return np.isin(labels, np.unique(labels[seeds])) & candidates
