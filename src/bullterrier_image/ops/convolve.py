"""Convolution of a picture with a 2-D kernel by FFT, at a cost that hardly grows with the kernel's size."""

from typing import Literal

import numpy as np
from scipy import fft

from bullterrier_image.ops.blur import NUMPY_PAD

Edge = Literal["reflect", "nearest", "mirror", "wrap"]


def convolve(image: np.ndarray, kernel: np.ndarray, *, edge: Edge) -> np.ndarray:
    """`image` (height, width) or (height, width, channels), floats, averaged with the weights of `kernel`, each
    channel on its own: output[y, x] is the sum over (i, j) of kernel[i + r, j + s] * image[y - i, x - j] for a
    kernel of (2r + 1) x (2s + 1) weights, so the kernel's middle element weighs the pixel itself and an asymmetric
    kernel is applied as a true convolution (flipped), not a correlation.

    `kernel` has an odd number of rows and columns, non-negative weights and a sum of 1. `edge` is what lies beyond the
    picture's edge, as in `bullterrier_image.ops.blur`: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).

    The result is within float32 rounding of the exact one and never outside the range of values `image` holds, in time
    that grows with the picture, not with the kernel. Returns a new array of the input's shape and dtype; the input and
    the kernel are left alone.
    """
    kernel = np.asarray(kernel, dtype=np.float64)
    height, width = image.shape[:2]
    reach_y, reach_x = kernel.shape[0] // 2, kernel.shape[1] // 2
    widths = [(reach_y, reach_y), (reach_x, reach_x)] + [(0, 0)] * (image.ndim - 2)
    padded = np.pad(image, widths, mode=NUMPY_PAD[edge])
    shape = (
        fft.next_fast_len(height + 2 * reach_y, real=True),
        fft.next_fast_len(width + 2 * reach_x, real=True),
    )
    # The kernel centred on sample (0, 0) of a circular buffer, so output sample i lines up with padded input sample i
    # and, the picture being padded by the kernel's reach on every side, nothing wraps around into what is kept.
    centred = np.zeros(shape)
    centred[np.ix_(np.arange(-reach_y, reach_y + 1) % shape[0], np.arange(-reach_x, reach_x + 1) % shape[1])] = kernel
    response = fft.rfft2(centred).astype(np.result_type(image.dtype, np.complex64))
    spectrum = fft.rfft2(padded, s=shape, axes=(0, 1))
    del padded
    spectrum *= np.expand_dims(response, tuple(range(2, image.ndim)))
    filtered = fft.irfft2(spectrum, s=shape, axes=(0, 1))[reach_y : reach_y + height, reach_x : reach_x + width]
    # The weights are positive and sum to 1, so the true result lies within the range of what it averages: clipping
    # to that range only moves float32 rounding error towards the truth.
    return np.clip(filtered, image.min(), image.max()).astype(image.dtype, copy=False)
