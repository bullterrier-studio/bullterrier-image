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
_SRGB_ENCODED_BREAK = 0.04045  # constant: encoded value where the sRGB linear segment ends (IEC 61966-2-1)
_SRGB_LINEAR_BREAK = 0.0031308  # constant: the same point in linear light (IEC 61966-2-1)


def _as_triplets(array: np.ndarray) -> np.ndarray:
    """Return ``array`` as float32 (a copy only if the dtype differs), after checking that its last axis is RGB."""
    array = np.asarray(array, dtype=np.float32)
    if array.ndim == 0 or array.shape[-1] != 3:  # constant: three colour channels
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
    rgb = _as_triplets(rgb)
    magnitude = np.abs(rgb)
    linear = np.where(
        magnitude <= _SRGB_ENCODED_BREAK,
        magnitude / 12.92,  # constant: slope of the sRGB linear segment (IEC 61966-2-1)
        ((magnitude + 0.055) / 1.055) ** 2.4,  # constant: sRGB power-law segment (IEC 61966-2-1)
    )
    return np.copysign(linear, rgb).astype(np.float32)


def linear_to_srgb(rgb: np.ndarray) -> np.ndarray:
    """Encode linear light as gamma-encoded sRGB; the exact inverse of `srgb_to_linear`.

    Args:
        rgb: linear-light values, last axis length 3, nominally 0..1. Values outside 0..1 are mirrored around zero, so
            negative inputs give negative outputs instead of NaN.

    Returns:
        float32 array of the same shape holding sRGB values, 0..1 for in-range input.
    """
    rgb = _as_triplets(rgb)
    magnitude = np.abs(rgb)
    encoded = np.where(
        magnitude <= _SRGB_LINEAR_BREAK,
        magnitude * 12.92,  # constant: slope of the sRGB linear segment (IEC 61966-2-1)
        1.055 * magnitude ** (1 / 2.4) - 0.055,  # constant: sRGB power-law segment (IEC 61966-2-1)
    )
    return np.copysign(encoded, rgb).astype(np.float32)


def rgb_to_hsv(rgb: np.ndarray) -> np.ndarray:
    """Convert sRGB to hue, saturation, value.

    Args:
        rgb: sRGB values, last axis length 3 (r, g, b), 0..1.

    Returns:
        float32 array of the same shape holding (h, s, v), each 0..1. Hue is a share of the full colour circle
        (0 = red, 1/3 = green, 2/3 = blue) in [0, 1); it is 0 for greys, whose hue is undefined. Saturation is 0 for
        greys and black.
    """
    rgb = _as_triplets(rgb)
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    value = rgb.max(axis=-1)
    chroma = value - rgb.min(axis=-1)
    grey = chroma == 0
    safe_chroma = np.where(grey, 1, chroma)  # the division result is discarded for greys
    # constant: the hue circle is six sectors, red to yellow to green to cyan to blue to magenta (standard HSV)
    sector = np.where(
        value == r,
        ((g - b) / safe_chroma) % 6,
        np.where(value == g, (b - r) / safe_chroma + 2, (r - g) / safe_chroma + 4),
    )
    hue = sector / 6  # constant: six sectors of the hue circle, as above; a grey has g - b = 0, so its hue is 0
    hue = np.where(hue >= 1, 0, hue)  # a red with a trace of blue rounds up to exactly 1, which is red again
    saturation = np.where(value == 0, 0, chroma / np.where(value == 0, 1, value))
    return np.stack([hue, saturation, value], axis=-1).astype(np.float32)


def hsv_to_rgb(hsv: np.ndarray) -> np.ndarray:
    """Convert hue, saturation, value to sRGB; the inverse of `rgb_to_hsv`.

    Args:
        hsv: last axis length 3 (h, s, v). Hue is a share of the full colour circle and wraps, so 1.25 and -0.75 both
            mean 0.25; saturation and value are 0..1.

    Returns:
        float32 array of the same shape holding sRGB (r, g, b), 0..1 for in-range saturation and value.
    """
    hsv = _as_triplets(hsv)
    hue, saturation, value = hsv[..., 0:1], hsv[..., 1:2], hsv[..., 2:3]  # constant: slices of the three channels
    # For channel offset n (5, 3, 1 for r, g, b) the channel is value, dipping to value*(1-saturation) over a
    # trapezoid of the hue circle: the standard branch-free HSV formula.
    k = (np.array([5, 3, 1], dtype=np.float32) + hue * 6) % 6  # constant: channel offsets and six sectors, standard HSV
    ramp = np.clip(np.minimum(k, 4 - k), 0, 1)  # constant: the trapezoid's plateau starts 4 sectors on, standard HSV
    return (value - value * saturation * ramp).astype(np.float32)
