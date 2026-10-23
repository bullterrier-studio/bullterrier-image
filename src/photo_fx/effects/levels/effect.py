"""The `levels` effect: black point, white point and midtone gamma, as in Photoshop's and Lightroom's Levels."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect

# constant: only keeps equal or crossed input points from dividing by zero; the span then counts as this tiny, so the
# picture is cut at `black` into the two output ends (a threshold).
_MIN_SPAN = 1e-6


class LevelsParams(EffectParams):
    """Input black and white points, midtone gamma and output black and white points."""

    black: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The input black point (0..1): values at or below it become the output black. Raising it darkens shadows "
        "and adds contrast.",
    )
    white: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The input white point (0..1): values at or above it become the output white. Lowering it brightens "
        "highlights and adds contrast. At or below `black` the picture is cut at `black` into the two output ends.",
    )
    gamma: float = param(
        1.0,
        unit="ratio",
        min=0.1,
        max=10,
        doc="The midtone gamma: 1 = linear, above 1 brightens the midtones, below 1 darkens them; black and white stay "
        "where they are (output = input ** (1 / gamma), as in Photoshop).",
    )
    output_black: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The darkest value that can be output (0..1): raising it lifts the blacks to a matte grey. Above "
        "`output_white` the tones are reversed.",
    )
    output_white: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightest value that can be output (0..1): lowering it dims the whites.",
    )


@effect(category="colour", params=LevelsParams)
def levels(rgb: np.ndarray, params: LevelsParams) -> np.ndarray:
    """Remap tones with an input black point, white point and midtone `gamma`, then an output range.

    Each channel is normalised between `black` and `white` and clipped to 0..1, raised to `1 / gamma`, and placed
    between `output_black` and `output_white`. The defaults change nothing.
    """
    span = max(params.white - params.black, _MIN_SPAN)
    x = np.clip((rgb - np.float32(params.black)) / np.float32(span), 0.0, 1.0)
    x = x ** np.float32(1.0 / params.gamma)
    return (np.float32(params.output_black) + x * np.float32(params.output_white - params.output_black)).astype(
        np.float32
    )
