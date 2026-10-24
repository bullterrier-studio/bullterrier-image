"""The `auto_levels` effect: stretch the picture to the full tonal range, like Photoshop's Auto Levels."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.tone import remap_levels


class AutoLevelsParams(EffectParams):
    """How much of each end is clipped, and whether the channels are stretched together."""

    clip: float = param(
        0.005,
        unit="fraction",
        min=0,
        max=0.45,
        doc="The share of values (0..0.45) that end up saturated at each end, pure black and pure white: 0.005 is "
        "0.5 percent, 0 stretches exactly from the darkest to the brightest value. Raising it gives more contrast "
        "and ignores stray dark or bright pixels, at the cost of detail in the extremes.",
    )
    mode: Literal["linked", "per_channel"] = param(
        "per_channel",
        doc="What is stretched: 'per_channel' stretches red, green and blue separately, which also removes a colour "
        "cast but shifts colours (Photoshop's Enhance Per Channel Contrast); 'linked' finds one black and one white "
        "point over all channels together and stretches them alike, so colours keep their relationships "
        "(Enhance Monochromatic Contrast).",
    )


@effect(category="colour", params=AutoLevelsParams)
def auto_levels(rgb: np.ndarray, params: AutoLevelsParams) -> np.ndarray:
    """Stretch the tones so the darkest values become black and the brightest white, ignoring `clip` at each end.

    The black and white points are read from the picture itself: the `clip` share of the darkest values and of the
    brightest values (counted per channel, or over all channels together when `mode` is 'linked') end up saturated,
    and the rest is spread linearly between. A channel with no range (every value equal) is left as it is.
    """
    # constant: one column of values per colour channel (red, green, blue), or a single column for all of them
    values = rgb.reshape(-1, 3) if params.mode == "per_channel" else rgb.reshape(-1, 1)
    count = values.shape[0]
    cut = int(params.clip * count)  # whole values dropped at each end
    ordered = np.partition(values, [cut, count - 1 - cut], axis=0)
    black, white = ordered[cut], ordered[count - 1 - cut]
    flat = white <= black  # nothing to stretch: keep the channel by using the identity points
    return remap_levels(
        rgb,
        black=tuple(np.where(flat, 0, black).tolist()),
        white=tuple(np.where(flat, 1, white).tolist()),
        gamma=1.0,  # constant: a linear stretch, so gamma 1 leaves the midtones where they fall
        output_black=0.0,  # constant: the stretch ends at pure black ...
        output_white=1.0,  # constant: ... and pure white
    )
