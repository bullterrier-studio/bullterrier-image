"""The `color_balance` effect: a colour pushed into the shadows, the midtones and the highlights."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance


class ColorBalanceParams(EffectParams):
    """Three colour pushes, one for the dark, middle and bright tones, that by default leave brightness alone."""

    shadows: Color = param(
        "#3a1bff",
        doc="The colour the dark parts are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' "
        "leaves them alone; a lighter or darker colour than that also lightens or darkens them unless "
        "`preserve_luminosity` is on.",
    )
    midtones: Color = param(
        "#808080",
        doc="The colour the middle tones are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' "
        "leaves them alone.",
    )
    highlights: Color = param(
        "#ff2bd6",
        doc="The colour the bright parts are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' "
        "leaves them alone.",
    )
    amount: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="How far each range is pushed toward its colour: 0 = off, 0.2 a hint, 0.6 strong, 1 moves a range by the "
        "full distance from mid grey to its colour.",
    )
    balance: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="Where the three ranges sit on the brightness scale (0..1): 0.5 splits it evenly, shadows below a quarter "
        "and highlights above three quarters. Raising it lets the shadows reach higher up and shrinks the highlights; "
        "lowering it does the reverse.",
    )
    softness: float = param(
        1.0,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How far neighbouring ranges overlap (0.01..1): 1 blends each into the next over half the brightness "
        "scale, with no pure midtones except at the centre; smaller makes sharper hand-overs and a wider band of "
        "pure midtones.",
    )
    preserve_luminosity: bool = param(
        True,
        doc="Keep each pixel's Rec. 709 luma while pushing colours: on, the brightness a push adds or removes is "
        "taken off or put back on all three channels, so only the colour changes (Photoshop's 'Preserve "
        "Luminosity'); off, a push toward a lighter or darker colour also lightens or darkens. Where a channel "
        "reaches 0 or 1 the luma is only kept as far as the clipping allows.",
        advanced=True,
    )


@effect(category="colour", params=ColorBalanceParams)
def color_balance(rgb: np.ndarray, params: ColorBalanceParams) -> np.ndarray:
    """Push the shadows, midtones and highlights each toward a colour: split toning, duotone, teal and orange grading.

    A pixel's tone is its Rec. 709 luma L of the sRGB-encoded picture. The shadow/midtone and midtone/highlight
    hand-overs are centred on `balance` / 2 and (1 + `balance`) / 2, each `softness` / 2 wide, and run along
    the smoothstep s(x) = 3x^2 - 2x^3 of their argument held to 0..1. Its weights are shadows 1 - lower, midtones
    lower - upper and highlights upper, so black is all shadows, mid grey (at the defaults) all midtones, white
    all highlights, and the three always add up to 1. Each channel moves by `amount` times the sum, over the three
    ranges, of the weight times (the range's colour - 0.5); the result is held to 0..1. With `preserve_luminosity`
    the luma the pushes changed is then restored by adding the same amount to every channel (before the hold).
    """
    luma = luminance(rgb)
    centre = params.balance / 2
    width = params.softness / 2
    lower = _handover(luma, centre, width)
    upper = _handover(luma, centre + 0.5, width)
    weights = (1 - lower, lower - upper, upper)  # shadows, midtones, highlights
    colours = (params.shadows, params.midtones, params.highlights)
    out = rgb
    for weight, colour in zip(weights, colours, strict=True):
        out = out + params.amount * weight[..., None] * (np.asarray(colour, np.float32) - 0.5)
    if params.preserve_luminosity:
        out = out + (luma - luminance(out))[..., None]
    return np.clip(out, 0.0, 1.0).astype(np.float32)


def _handover(luma: np.ndarray, centre: float, width: float) -> np.ndarray:
    """0 below `centre` - `width` / 2, 1 above `centre` + `width` / 2, and a smoothstep in between."""
    x = np.clip((luma - centre) / width + 0.5, 0.0, 1.0)
    return x * x * (3 - 2 * x)  # constant: the cubic Hermite smoothstep, 3x^2 - 2x^3
