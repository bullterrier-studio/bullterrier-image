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
_SRGB_LINEAR_BREAK = 0.0031308  # constant: the same point in linear light (IEC 61966-2-1)


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


def linear_to_srgb(rgb: np.ndarray) -> np.ndarray:
    """Encode linear light as gamma-encoded sRGB; the exact inverse of `srgb_to_linear`.

    Args:
        rgb: linear-light values, last axis length 3, nominally 0..1. Values outside 0..1 are mirrored around zero, so
            negative inputs give negative outputs instead of NaN.

    Returns:
        float32 array of the same shape holding sRGB values, 0..1 for in-range input.
    """
    rgb = np.asarray(rgb, dtype=np.float32)
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
    rgb = np.asarray(rgb, dtype=np.float32)
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
    hsv = np.asarray(hsv, dtype=np.float32)
    hue, saturation, value = hsv[..., 0:1], hsv[..., 1:2], hsv[..., 2:3]
    # For channel offset n (5, 3, 1 for r, g, b) the channel is value, dipping to value*(1-saturation) over a
    # trapezoid of the hue circle: the standard branch-free HSV formula.
    k = (np.array([5, 3, 1], dtype=np.float32) + hue * 6) % 6  # constant: channel offsets and six sectors, standard HSV
    ramp = np.clip(np.minimum(k, 4 - k), 0, 1)  # constant: the trapezoid's plateau starts 4 sectors on, standard HSV
    return (value - value * saturation * ramp).astype(np.float32)


# JFIF full-range YCbCr (ITU-T T.871): Y is the luma of the Rec. 601 weights, Cb and Cr are the blue and red differences
# scaled to span the full range, centred on 128 of 255.
_RGB_TO_YCBCR = np.array(
    [[0.299, 0.587, 0.114], [-0.168736, -0.331264, 0.5], [0.5, -0.418688, -0.081312]], dtype=np.float32
)  # constant: the JFIF matrix, ITU-T T.871
_YCBCR_TO_RGB = np.linalg.inv(_RGB_TO_YCBCR).astype(np.float32)
_CHROMA_CENTRE = np.array([0, 128 / 255, 128 / 255], dtype=np.float32)  # constant: JFIF centres Cb and Cr on 128 of 255


def rgb_to_ycbcr(rgb: np.ndarray) -> np.ndarray:
    """Convert sRGB to the JFIF YCbCr of JPEG files (ITU-T T.871), as fractions of the 8-bit range.

    Args:
        rgb: sRGB values, last axis length 3, 0..1.

    Returns:
        float32 array of the same shape holding (Y, Cb, Cr), each 0..1 for in-range input: Y is the luma (the
        Rec. 601 weights 0.299, 0.587, 0.114), and Cb and Cr, the blue and red differences, are centred on 128/255
        (a grey has both there). Red's Cr and blue's Cb reach 255.5/255, just above 1; nothing is clipped.
    """
    return (np.asarray(rgb, dtype=np.float32) @ _RGB_TO_YCBCR.T + _CHROMA_CENTRE).astype(np.float32)


def ycbcr_to_rgb(ycbcr: np.ndarray) -> np.ndarray:
    """Convert JFIF YCbCr, as fractions of the 8-bit range, back to sRGB; the inverse of `rgb_to_ycbcr`.

    Args:
        ycbcr: (Y, Cb, Cr) on the last axis, as `rgb_to_ycbcr` returns them.

    Returns:
        float32 array of the same shape holding sRGB (r, g, b), 0..1 for the YCbCr of in-range colours; not clipped.
    """
    return ((np.asarray(ycbcr, dtype=np.float32) - _CHROMA_CENTRE) @ _YCBCR_TO_RGB.T).astype(np.float32)


# NTSC YIQ with the FCC coefficients (47 CFR 73.682): Y the luma, I the orange-cyan and Q the green-magenta axis of the
# chroma, which a grey leaves at zero.
_RGB_TO_YIQ = np.array(
    [[0.299, 0.587, 0.114], [0.5959, -0.2746, -0.3213], [0.2115, -0.5227, 0.3112]], dtype=np.float32
)  # constant: the FCC YIQ matrix, as four-digit coefficients
_YIQ_TO_RGB = np.linalg.inv(_RGB_TO_YIQ).astype(np.float32)


def rgb_to_yiq(rgb: np.ndarray) -> np.ndarray:
    """Convert sRGB to the NTSC YIQ of television (FCC 47 CFR 73.682 coefficients).

    Args:
        rgb: sRGB values, last axis length 3, 0..1.

    Returns:
        float32 array of the same shape holding (Y, I, Q): Y is 0..1 (the Rec. 601 luma weights 0.299, 0.587, 0.114),
        I about -0.6..0.6 and Q about -0.53..0.53, both 0 for a grey. Nothing is clipped.
    """
    return (np.asarray(rgb, dtype=np.float32) @ _RGB_TO_YIQ.T).astype(np.float32)


def yiq_to_rgb(yiq: np.ndarray) -> np.ndarray:
    """Convert NTSC YIQ back to sRGB; the inverse of `rgb_to_yiq`.

    Args:
        yiq: (Y, I, Q) on the last axis, as `rgb_to_yiq` returns them.

    Returns:
        float32 array of the same shape holding sRGB (r, g, b), 0..1 for the YIQ of in-range colours; not clipped.
    """
    return (np.asarray(yiq, dtype=np.float32) @ _YIQ_TO_RGB.T).astype(np.float32)


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
