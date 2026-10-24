"""The `selective_color` effect: shift hue, saturation and lightness for one range of colours only."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.colorspace import hsv_to_rgb, rgb_to_hsv

# constant: only keeps a hard-edged range (feather 0) from dividing by zero; a pixel past the edge still gets weight 0.
_NO_FEATHER = 1e-6


class SelectiveColorParams(EffectParams):
    """A range of hues (a centre, a width and a feather) and the changes made to the colours inside it."""

    hue: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        doc="Centre of the range on the colour wheel, 0..360 degrees: 0 red, 60 yellow, 120 green, 180 cyan, 240 blue, "
        "300 magenta; 360 is red again.",
    )
    width: float = param(
        60.0,
        unit="degrees",
        min=0,
        max=360,
        doc="Full width of the range, 0..360 degrees, centred on `hue`; colours inside it are changed at full "
        "strength. 360 takes in every colour; raising it widens the range.",
    )
    feather: float = param(
        30.0,
        unit="degrees",
        min=0,
        max=180,
        doc="How far beyond each edge of the range the change fades out, 0..180 degrees: the weight falls in a "
        "straight line from full at the edge to none that far away. 0 gives a hard edge; raising it blends the range "
        "into its neighbours.",
    )
    hue_shift: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="How far the selected hues are turned round the colour wheel, -180..180 degrees; positive goes red toward "
        "yellow, green, blue. 0 keeps the hue.",
    )
    saturation: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=3,
        doc="Gain on the saturation of the selected colours: 1 keeps it, 0 turns them grey, above 1 makes them more "
        "vivid (stopping at full saturation).",
    )
    lightness: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Brightness (HSV value) of the selected colours, -1..1: -1 takes them all the way to black, +1 all the way "
        "to the full value, 0 keeps it; a value of 0.5 closes half the gap in that direction.",
    )


@effect(category="colour", params=SelectiveColorParams)
def selective_color(rgb: np.ndarray, params: SelectiveColorParams) -> np.ndarray:
    """Shift the hue, saturation and lightness of the colours in one hue range, leaving every other pixel untouched.

    Works in HSV on the gamma-encoded values. A pixel's weight is its place in the range (1 within `width` of the
    `hue` centre, falling linearly to 0 over `feather` degrees; the wheel wraps round) times its HSV saturation, so a
    grey, which has no hue, is never selected and a pale colour is changed less than a vivid one. Pixels of weight 0
    come back bit for bit. A selected pixel moves by weight times each change: `hue_shift` degrees, saturation times
    `saturation` and value toward black or full by `lightness`. Saturation and value stay inside 0..1 by
    construction, so the result needs no clipping.
    """
    hsv = rgb_to_hsv(rgb)
    hue_degrees = hsv[..., 0] * 360  # constant: degrees in a full turn of the colour wheel
    distance = np.abs((hue_degrees - params.hue + 180) % 360 - 180)  # constant: half and full turn, in degrees
    outside_by = distance - params.width / 2
    weight = np.clip(1 - outside_by / max(params.feather, _NO_FEATHER), 0, 1) * hsv[..., 1]
    saturation = np.clip(hsv[..., 1] * (1 + weight * (params.saturation - 1)), 0, 1)
    gap = np.where(params.lightness > 0, 1 - hsv[..., 2], hsv[..., 2])  # room to move toward full or toward black
    value = hsv[..., 2] + weight * params.lightness * gap
    hue = (hue_degrees + weight * params.hue_shift) / 360  # constant: degrees in a full turn of the colour wheel
    adjusted = hsv_to_rgb(np.stack([hue, saturation, value], axis=-1))
    return np.where(weight[..., None] > 0, adjusted, rgb).astype(np.float32)
