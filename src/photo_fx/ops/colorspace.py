"""Colour-space conversions and linear-light processing.

Every function takes an array whose last axis has length 3 (any leading shape: one colour, an image, a stack of
images) and returns a new float32 array of the same shape; the input is never modified and may have any numeric
dtype. A last axis of any other length raises ``ValueError``.

Unless stated otherwise, "RGB" means gamma-encoded sRGB with channels in 0..1, the format used everywhere in photo-fx.
Values outside 0..1 are not rejected or clipped: the conversions carry them through (the sRGB transfer function is
extended symmetrically around zero), so callers decide how to handle out-of-gamut colours.
"""

import numpy as np

# sRGB transfer function, IEC 61966-2-1.
_SRGB_ENCODED_BREAK = 0.04045  # encoded value where the linear segment ends
_SRGB_LINEAR_BREAK = 0.0031308  # the same point in linear light


def _as_rgb(array: np.ndarray) -> np.ndarray:
    """Return ``array`` as float32 (a copy only if the dtype differs), after checking that its last axis is RGB."""
    array = np.asarray(array, dtype=np.float32)
    if array.ndim == 0 or array.shape[-1] != 3:
        raise ValueError(f"expected an array whose last axis has length 3 (RGB channels), got shape {array.shape}")
    return array


def srgb_to_linear(rgb: np.ndarray) -> np.ndarray:
    """Decode gamma-encoded sRGB into linear light with the sRGB transfer function (IEC 61966-2-1).

    Args:
        rgb: sRGB values, last axis length 3, nominally 0..1 (0 = black, 1 = full intensity). Values outside 0..1 are
            mirrored around zero, so negative inputs give negative outputs instead of NaN.

    Returns:
        float32 array of the same shape holding linear-light values, 0..1 for in-range input.
    """
    rgb = _as_rgb(rgb)
    magnitude = np.abs(rgb)
    linear = np.where(
        magnitude <= _SRGB_ENCODED_BREAK,
        magnitude / 12.92,
        ((magnitude + 0.055) / 1.055) ** 2.4,
    )
    return np.copysign(linear, rgb).astype(np.float32)


def linear_to_srgb(rgb: np.ndarray) -> np.ndarray:
    """Encode linear light as gamma-encoded sRGB; the exact inverse of `srgb_to_linear`.

    Args:
        rgb: linear-light values, last axis length 3, nominally 0..1. Values outside 0..1 are mirrored around zero, so
            negative inputs (for example from an unclipped OKLab conversion) give negative outputs instead of NaN.

    Returns:
        float32 array of the same shape holding sRGB values, 0..1 for in-range input.
    """
    rgb = _as_rgb(rgb)
    magnitude = np.abs(rgb)
    encoded = np.where(
        magnitude <= _SRGB_LINEAR_BREAK,
        magnitude * 12.92,
        1.055 * magnitude ** (1 / 2.4) - 0.055,
    )
    return np.copysign(encoded, rgb).astype(np.float32)
