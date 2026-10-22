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

# OKLab (Bjorn Ottosson, 2020): linear sRGB -> cone responses, then a cube root, then to (L, a, b). The inverses are
# computed rather than typed in so a round trip is exact whatever the rounding of the published constants.
_LINEAR_RGB_TO_LMS = np.array(
    [
        [0.4122214708, 0.5363325363, 0.0514459929],
        [0.2119034982, 0.6806995451, 0.1073969566],
        [0.0883024619, 0.2817188376, 0.6299787005],
    ]
)
_LMS_ROOT_TO_LAB = np.array(
    [
        [0.2104542553, 0.7936177850, -0.0040720468],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757660],
    ]
)
# Applied to row vectors (x @ M.T), so each is stored already transposed.
_RGB_TO_LMS_T = _LINEAR_RGB_TO_LMS.T.astype(np.float32)
_LMS_ROOT_TO_LAB_T = _LMS_ROOT_TO_LAB.T.astype(np.float32)
_LMS_TO_RGB_T = np.linalg.inv(_LINEAR_RGB_TO_LMS).T.astype(np.float32)
_LAB_TO_LMS_ROOT_T = np.linalg.inv(_LMS_ROOT_TO_LAB).T.astype(np.float32)


def _as_triplets(array: np.ndarray) -> np.ndarray:
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
    rgb = _as_triplets(rgb)
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
    rgb = _as_triplets(rgb)
    magnitude = np.abs(rgb)
    encoded = np.where(
        magnitude <= _SRGB_LINEAR_BREAK,
        magnitude * 12.92,
        1.055 * magnitude ** (1 / 2.4) - 0.055,
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
    sector = np.where(
        value == r,
        ((g - b) / safe_chroma) % 6,
        np.where(value == g, (b - r) / safe_chroma + 2, (r - g) / safe_chroma + 4),
    )
    hue = np.where(grey, 0, sector / 6)
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
    hue, saturation, value = hsv[..., 0:1], hsv[..., 1:2], hsv[..., 2:3]
    # For channel offset n (5, 3, 1 for r, g, b) the channel is value, dipping to value*(1-saturation) over a
    # trapezoid of the hue circle: the standard branch-free HSV formula.
    k = (np.array([5, 3, 1], dtype=np.float32) + hue * 6) % 6
    ramp = np.clip(np.minimum(k, 4 - k), 0, 1)
    return (value - value * saturation * ramp).astype(np.float32)


def rgb_to_oklab(rgb: np.ndarray) -> np.ndarray:
    """Convert sRGB to OKLab, a perceptually uniform space (Bjorn Ottosson, 2020).

    Args:
        rgb: sRGB values, last axis length 3 (r, g, b), 0..1.

    Returns:
        float32 array of the same shape holding (L, a, b). L is perceptual lightness, 0 (black) to 1 (white); a runs
        green (negative) to red (positive) and b blue (negative) to yellow (positive), roughly -0.4..0.4 for sRGB
        colours. Greys have a = b = 0.
    """
    lms = srgb_to_linear(rgb) @ _RGB_TO_LMS_T
    return (np.cbrt(lms) @ _LMS_ROOT_TO_LAB_T).astype(np.float32)


def oklab_to_rgb(lab: np.ndarray) -> np.ndarray:
    """Convert OKLab to sRGB; the inverse of `rgb_to_oklab`.

    Args:
        lab: last axis length 3 (L, a, b) as returned by `rgb_to_oklab`; L is 0..1, a and b are unbounded.

    Returns:
        float32 array of the same shape holding sRGB (r, g, b). The result is NOT clipped: colours outside the sRGB
        gamut come back with channels below 0 or above 1, and the caller decides how to bring them into range.
    """
    lms = (_as_triplets(lab) @ _LAB_TO_LMS_ROOT_T) ** 3
    return linear_to_srgb(lms @ _LMS_TO_RGB_T)
