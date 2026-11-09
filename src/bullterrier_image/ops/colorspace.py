"""Colour-space conversions and linear-light processing.

Every function takes an array whose last axis has length 3 (any leading shape: one colour, an image, a stack of
images) and returns a new float32 array of the same shape; the input is never modified and may have any numeric
dtype.

Unless stated otherwise, "RGB" means gamma-encoded sRGB with channels in 0..1, the format used everywhere in
bullterrier-image. Values outside 0..1 are not rejected or clipped: the conversions carry them through (the sRGB
transfer function is extended symmetrically around zero), so callers decide how to handle out-of-gamut colours.
"""

import numpy as np

# sRGB transfer function, IEC 61966-2-1.
_SRGB_ENCODED_BREAK = 0.04045  # constant: encoded value where the sRGB linear segment ends (IEC 61966-2-1)


def srgb_to_linear(rgb: np.ndarray) -> np.ndarray:
    """Decode gamma-encoded sRGB into linear light with the sRGB transfer function (IEC 61966-2-1).

    Args:
        rgb: sRGB values, last axis length 3, nominally 0..1 (0 = black, 1 = full intensity). Values outside 0..1 are
            mirrored around zero, so negative inputs give negative outputs instead of NaN.

    Returns:
        float32 array of the same shape holding linear-light values, 0..1 for in-range input.
    """
    rgb = np.asarray(rgb, dtype=np.float32)
    magnitude = np.abs(rgb)
    linear = np.where(
        magnitude <= _SRGB_ENCODED_BREAK,
        magnitude / 12.92,  # constant: slope of the sRGB linear segment (IEC 61966-2-1)
        ((magnitude + 0.055) / 1.055) ** 2.4,  # constant: sRGB power-law segment (IEC 61966-2-1)
    )
    return np.copysign(linear, rgb).astype(np.float32)


def rgb_to_oklab(rgb: np.ndarray) -> np.ndarray:
    """Convert sRGB to Oklab (Bjorn Ottosson, 2020), a space in which equal distances look about equally different.

    Args:
        rgb: sRGB values, last axis length 3, 0..1.

    Returns:
        float32 array of the same shape holding (L, a, b): L the lightness, 0 for black and 1 for white, a the
        green-red and b the blue-yellow axis, both about 0 for a grey and within about -0.4..0.4 for sRGB colours.
    """
    # constant: Ottosson's matrix from linear sRGB to cone responses
    to_cones = np.array(
        [
            [0.4122214708, 0.5363325363, 0.0514459929],
            [0.2119034982, 0.6806995451, 0.1073969566],
            [0.0883024619, 0.2817188376, 0.6299787005],
        ],
        dtype=np.float32,
    )
    # constant: Ottosson's matrix from compressed cone responses to lightness and the two colour axes
    to_lab = np.array(
        [
            [0.2104542553, 0.7936177850, -0.0040720468],
            [1.9779984951, -2.4285922050, 0.4505937099],
            [0.0259040371, 0.7827717662, -0.8086757660],
        ],
        dtype=np.float32,
    )
    return (np.cbrt(srgb_to_linear(rgb) @ to_cones.T) @ to_lab.T).astype(np.float32)
