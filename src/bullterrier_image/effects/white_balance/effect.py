"""The `white_balance` effect: correct a colour cast along the warm-cool and green-magenta axes."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.colorspace import linear_to_srgb, srgb_to_linear


class WhiteBalanceParams(EffectParams):
    """Von Kries scaling of red, green and blue in linear light, set by a warm-cool and a green-magenta control."""

    temperature: float = param(
        0.0,
        unit="stops",
        min=-2,
        max=2,
        doc="Warm-cool shift in stops (-2..2): the difference in stops between the red and the blue gain, shared "
        "evenly (red up and blue down by half of it each). Positive warms the picture (more orange), negative cools "
        "it (more blue), 0 leaves the colour balance alone. Use a negative value to correct a picture shot under warm "
        "light.",
    )
    tint: float = param(
        0.0,
        unit="stops",
        min=-1,
        max=1,
        doc="Green-magenta shift in stops (-1..1): the green gain is multiplied by 2 to the power of minus tint. "
        "Positive adds magenta (less green), negative adds green, 0 leaves the balance alone.",
    )


@effect(category="colour", params=WhiteBalanceParams)
def white_balance(rgb: np.ndarray, params: WhiteBalanceParams) -> np.ndarray:
    """Shift the colour balance by `temperature` (warm-cool) and `tint` (green-magenta), in stops of linear light.

    This is von Kries scaling: the picture is decoded to linear light (sRGB, IEC 61966-2-1), each channel is multiplied
    by a gain, and it is encoded again. The gains are red 2**(temperature/2), green 2**-tint and blue
    2**(-temperature/2), divided by the largest of the three, so the result is never brighter than the input, never
    clips, and a white pixel keeps one full channel instead of turning into a coloured cast. The defaults give gains
    of 1: no change.
    """
    half = params.temperature / 2  # constant: the temperature stops are shared evenly between red and blue
    gains = np.exp2(np.array([half, -params.tint, -half], dtype=np.float32))
    gains /= gains.max()
    return np.clip(linear_to_srgb(srgb_to_linear(rgb) * gains), 0.0, 1.0)
