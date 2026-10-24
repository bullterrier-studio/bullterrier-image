"""The `curves` effect: a tone curve through control points (master and per colour channel), as in Photoshop's and
Lightroom's Curves."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.tone import monotone_curve

# constant: where the control points sit on the input axis: black, a quarter, the middle, three quarters and white
_INPUTS = (0.0, 0.25, 0.5, 0.75, 1.0)
# constant: a channel curve starts at black and ends at white; only its middle three points move
_BLACK, _WHITE = 0.0, 1.0


class CurvesParams(EffectParams):
    """The heights of the control points of the master curve and of the red, green and blue curves."""

    black: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The master curve's output for pure black, on the input axis at 0 (0..1): raising it lifts the blacks "
        "to a matte grey.",
    )
    shadows: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The master curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged): lower "
        "darkens the shadows, higher opens them up.",
    )
    midtones: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The master curve's output for the middle input, 0.5 (0..1; 0.5 = unchanged): lower darkens the "
        "midtones, higher brightens them.",
    )
    highlights: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The master curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged): lower "
        "dims the highlights, higher makes them brighter.",
    )
    white: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The master curve's output for pure white, at 1 on the input axis (0..1): lowering it dims the whites.",
    )
    red_shadows: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The red curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied after "
        "the master curve: raising it adds red to the shadows, lowering it takes red away. Its ends stay at "
        "black and white.",
        advanced=True,
    )
    red_midtones: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The red curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the "
        "master curve: raising it adds red to the midtones, lowering it takes red away. Its ends stay at "
        "black and white.",
        advanced=True,
    )
    red_highlights: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The red curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), applied "
        "after the master curve: raising it adds red to the highlights, lowering it takes red away. Its ends "
        "stay at black and white.",
        advanced=True,
    )
    green_shadows: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The green curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied "
        "after the master curve: raising it adds green to the shadows, lowering it takes green away. Its ends "
        "stay at black and white.",
        advanced=True,
    )
    green_midtones: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The green curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the "
        "master curve: raising it adds green to the midtones, lowering it takes green away. Its ends stay at "
        "black and white.",
        advanced=True,
    )
    green_highlights: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The green curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), "
        "applied after the master curve: raising it adds green to the highlights, lowering it takes green "
        "away. Its ends stay at black and white.",
        advanced=True,
    )
    blue_shadows: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The blue curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied "
        "after the master curve: raising it adds blue to the shadows, lowering it takes blue away. Its ends "
        "stay at black and white.",
        advanced=True,
    )
    blue_midtones: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The blue curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the "
        "master curve: raising it adds blue to the midtones, lowering it takes blue away. Its ends stay at "
        "black and white.",
        advanced=True,
    )
    blue_highlights: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The blue curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), applied "
        "after the master curve: raising it adds blue to the highlights, lowering it takes blue away. Its "
        "ends stay at black and white.",
        advanced=True,
    )


@effect(category="colour", params=CurvesParams)
def curves(rgb: np.ndarray, params: CurvesParams) -> np.ndarray:
    """Bend tones along smooth curves through control points: a master curve, then one per colour channel.

    The master curve has five points at input 0, 0.25, 0.5, 0.75 and 1 (`black`, `shadows`, `midtones`, `highlights`,
    `white`) and is applied to all channels; each channel curve then has its own three middle points between a fixed
    black and white. A curve passes exactly through its points and is a monotone cubic (PCHIP): it never overshoots
    its neighbouring points, so rising points never darken a brighter tone. The defaults are the straight line.
    """
    master = (params.black, params.shadows, params.midtones, params.highlights, params.white)
    tones = monotone_curve(_INPUTS, master, rgb)
    heights = (
        (params.red_shadows, params.red_midtones, params.red_highlights),
        (params.green_shadows, params.green_midtones, params.green_highlights),
        (params.blue_shadows, params.blue_midtones, params.blue_highlights),
    )
    return np.stack(
        [monotone_curve(_INPUTS, (_BLACK, *middle, _WHITE), tones[..., index]) for index, middle in enumerate(heights)],
        axis=-1,
    )
