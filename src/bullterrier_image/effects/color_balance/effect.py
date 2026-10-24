"""The `color_balance` effect: separate red, green and blue shifts for the shadows, midtones and highlights."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance


class ColorBalanceParams(EffectParams):
    """Colour shifts for the dark, middle and bright tones, each a shift of red, green and blue."""

    shadows_red: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the red channel in the darkest tones, in 0..1 picture units at full weight (pure black): "
        "+0.1 adds a tenth of the range, -0.1 removes it (towards cyan). 0 = off.",
    )
    shadows_green: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the green channel in the darkest tones, at full weight (pure black): positive adds green, "
        "negative removes it (towards magenta). 0 = off.",
    )
    shadows_blue: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the blue channel in the darkest tones, at full weight (pure black): positive adds blue, "
        "negative removes it (towards yellow). 0 = off.",
    )
    midtones_red: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the red channel in the middle tones, at full weight (mid grey): positive adds red, negative "
        "removes it (towards cyan). 0 = off.",
    )
    midtones_green: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the green channel in the middle tones, at full weight (mid grey): positive adds green, "
        "negative removes it (towards magenta). 0 = off.",
    )
    midtones_blue: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the blue channel in the middle tones, at full weight (mid grey): positive adds blue, "
        "negative removes it (towards yellow). 0 = off.",
    )
    highlights_red: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the red channel in the brightest tones, at full weight (pure white): positive adds red, "
        "negative removes it (towards cyan). 0 = off.",
    )
    highlights_green: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the green channel in the brightest tones, at full weight (pure white): positive adds green, "
        "negative removes it (towards magenta). 0 = off.",
    )
    highlights_blue: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="Shift of the blue channel in the brightest tones, at full weight (pure white): positive adds blue, "
        "negative removes it (towards yellow). 0 = off.",
    )
    preserve_luminosity: bool = param(
        True,
        doc="Keep each pixel's Rec. 709 luma while shifting colours: on, the brightness the shifts add or remove is "
        "taken off or put back on all three channels, so only the colour changes (Photoshop's 'Preserve "
        "Luminosity'); off, the shifts also lighten or darken. Where a channel reaches 0 or 1 the luma is only "
        "kept as far as the clipping allows.",
        advanced=True,
    )


@effect(category="colour", params=ColorBalanceParams)
def color_balance(rgb: np.ndarray, params: ColorBalanceParams) -> np.ndarray:
    """Shift red, green and blue separately in the shadows, the midtones and the highlights.

    A pixel's tone is its Rec. 709 luma L of the sRGB-encoded picture. Its weights are shadows 1 - s(2L), midtones
    s(2L) - s(2L - 1) and highlights s(2L - 1), where s is the smoothstep 3x^2 - 2x^3 of its argument held to 0..1:
    black is all shadows, mid grey all midtones, white all highlights, the three always add up to 1 and change
    smoothly. Each channel moves by the sum, over the three tones, of the weight times that tone's shift for the
    channel; the result is held to 0..1. With `preserve_luminosity` the luma the shifts changed is then restored
    by adding the same amount to every channel (before the hold to 0..1).
    """
    luma = luminance(rgb)
    upper = _smoothstep(2 * luma - 1)
    lower = _smoothstep(2 * luma)
    weights = (1 - lower, lower - upper, upper)  # shadows, midtones, highlights
    shifts = np.array(
        [
            [params.shadows_red, params.shadows_green, params.shadows_blue],
            [params.midtones_red, params.midtones_green, params.midtones_blue],
            [params.highlights_red, params.highlights_green, params.highlights_blue],
        ],
        np.float32,
    )
    out = rgb + sum(weight[..., None] * shift for weight, shift in zip(weights, shifts, strict=True))
    if params.preserve_luminosity:
        out = out + (luma - luminance(out))[..., None]
    return np.clip(out, 0.0, 1.0).astype(np.float32)


def _smoothstep(x: np.ndarray) -> np.ndarray:
    """The smoothstep 3x^2 - 2x^3 of `x` held to 0..1: 0 below 0, 1 above 1, and flat at both ends."""
    x = np.clip(x, 0.0, 1.0)
    return x * x * (3 - 2 * x)  # constant: the cubic Hermite smoothstep, 3x^2 - 2x^3
