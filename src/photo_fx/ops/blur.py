"""Gaussian blur of a picture, at a cost that hardly grows with the blur's size."""

from typing import Literal

import numpy as np
from scipy import fft, ndimage

Mode = Literal["reflect", "constant", "nearest", "mirror", "wrap"]

# constant: kernel radius in px from which FFT beats direct convolution (measured: equal at 12 MP, FFT ahead at 1 MP)
_FFT_FROM_RADIUS = 32
# scipy's edge modes under numpy's names for the same extension (scipy 'reflect' repeats the edge pixel, as numpy's
# 'symmetric' does; scipy 'mirror' does not, as numpy's 'reflect' does not)
_NUMPY_PAD: dict[Mode, Literal["symmetric", "reflect", "edge", "wrap", "constant"]] = {
    "reflect": "symmetric",
    "mirror": "reflect",
    "nearest": "edge",
    "wrap": "wrap",
    "constant": "constant",
}


def gaussian_blur(
    image: np.ndarray, sigma: float, *, truncate: float, mode: Mode, constant_value: float | None = None
) -> np.ndarray:
    """`scipy.ndimage.gaussian_filter` over the two picture axes of a (height, width) or (height, width, channels)
    array, each channel on its own, at a cost that hardly grows with `sigma`.

    `sigma` is the Gaussian's standard deviation in px (0 = a copy), `truncate` where its kernel is cut off, in sigmas
    (the kernel reaches `int(truncate * sigma + 0.5)` px), `mode` what lies beyond the picture's edge, as in scipy:
    'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel
    repeated), 'wrap' (the opposite edge) or 'constant' (`constant_value`, which only that mode needs).

    A kernel reaching less than 32 px gives exactly scipy's result. A longer one gives scipy's result within 1e-5 of
    the largest magnitude in the picture and `constant_value` (float32 rounding; about 4e-7 measured on 0..1
    pictures), never outside the range they span, in time that grows with the picture, not with the kernel. Returns a
    new array of the input's shape and dtype; the input is left alone.
    """
    if mode == "constant" and constant_value is None:
        raise ValueError("gaussian_blur: mode 'constant' needs a constant_value")
    beyond = {} if constant_value is None else {"cval": constant_value}
    radius = int(truncate * float(sigma) + 0.5)  # how scipy sizes its kernel
    if radius < _FFT_FROM_RADIUS:
        sigmas = (sigma, sigma) + (0.0,) * (image.ndim - 2)  # a zero sigma leaves the channel axis alone
        return ndimage.gaussian_filter(image, sigmas, mode=mode, truncate=truncate, **beyond)
    offsets = np.arange(-radius, radius + 1, dtype=np.float64)
    kernel = np.exp(-0.5 * (offsets / sigma) ** 2)
    kernel /= kernel.sum()
    blurred = _convolve_axis(image, kernel, mode, constant_value, axis=0)
    blurred = _convolve_axis(blurred, kernel, mode, constant_value, axis=1)
    # The weights are positive and sum to 1, so the true result lies within the range of what it averages: clipping
    # to that range only moves rounding error towards the truth.
    low, high = float(image.min()), float(image.max())
    if constant_value is not None and mode == "constant":
        low, high = min(low, constant_value), max(high, constant_value)
    return np.clip(blurred, low, high).astype(image.dtype, copy=False)


def _convolve_axis(
    array: np.ndarray, kernel: np.ndarray, mode: Mode, constant_value: float | None, *, axis: int
) -> np.ndarray:
    """`array` convolved along `axis` with the odd-length, symmetric `kernel`, by FFT over the edge extension `mode`.

    The extension is one kernel radius on each side, so the picture's own samples never wrap around; the result is a
    view of a larger array, so the caller copies it once it is done.
    """
    radius = (len(kernel) - 1) // 2
    length = array.shape[axis]
    widths = [(0, 0)] * array.ndim
    widths[axis] = (radius, radius)
    if mode == "constant" and constant_value is not None:
        padded = np.pad(array, widths, mode="constant", constant_values=constant_value)
    else:
        padded = np.pad(array, widths, mode=_NUMPY_PAD[mode])
    size = fft.next_fast_len(length + 2 * radius, real=True)
    # The kernel centred on sample 0 of a circular buffer, so output sample i lines up with padded input sample i.
    centred = np.zeros(size)
    centred[: radius + 1] = kernel[radius:]
    centred[size - radius :] = kernel[:radius]
    response = fft.rfft(centred).astype(np.result_type(array.dtype, np.complex64))
    spectrum = fft.rfft(padded, n=size, axis=axis)
    del padded
    spectrum *= np.expand_dims(response, tuple(range(1, array.ndim - axis)))
    filtered = fft.irfft(spectrum, n=size, axis=axis)
    del spectrum
    inside = [slice(None)] * array.ndim
    inside[axis] = slice(radius, radius + length)
    return filtered[tuple(inside)]
