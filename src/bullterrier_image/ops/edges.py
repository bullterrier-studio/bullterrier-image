"""Edges: how fast a picture's brightness changes at each pixel."""

import numpy as np
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
