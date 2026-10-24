"""The `signal_noise` effect: bands of static and short dropout streaks, as from a failing signal."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect


class SignalNoiseParams(EffectParams):
    """The settings of `signal_noise`: the static bands, the dropout streaks and the random seed."""

    bands: int = param(
        3,
        unit="count",
        min=0,
        max=200,
        doc="How many horizontal bands of static cross the picture, 0 to 200; 0 = none. Bands land at random rows "
        "and may overlap.",
    )
    band_height: float = param(
        0.04,
        unit="fraction",
        min=0,
        max=1,
        doc="The tallest a band of static can be, as a fraction 0 to 1 of the picture's height. Each band is between "
        "half this and this tall (never under 1 px); raising it gives thicker bands.",
    )
    static: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=1,
        doc="How much static a band holds, 0 to 1: the share of each pixel in a band that is replaced by a random "
        "grey. 0 leaves the bands invisible, 1 fills them with pure snow.",
    )
    dropouts: int = param(
        12,
        unit="count",
        min=0,
        max=2000,
        doc="How many dropout streaks, 0 to 2000: short horizontal marks where the signal was lost and the tape "
        "shows `color`. 0 = none. Raising it covers more of the picture.",
    )
    color: Color = param(
        "#ffffff",
        doc="The colour of a dropout streak: '#rrggbb', '#rgb' or [r, g, b] 0..1. White is the usual look of a "
        "tape dropout; black gives dead gaps.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the bands and streaks fall and the grey of the static: the same seed always gives "
        "the same damage, any other a different one.",
    )
    dropout_length: float = param(
        0.12,
        unit="fraction",
        min=0,
        max=1,
        doc="The longest a dropout streak can be, as a fraction 0 to 1 of the picture's width. Each streak is between "
        "half this and this long (never under 1 px).",
        advanced=True,
    )
    dropout_height: int = param(
        2,
        unit="px",
        min=1,
        max=100,
        doc="How many rows tall a dropout streak is, 1 to 100 px, tuned for a picture about 480 px high. A streak "
        "that would cross the bottom edge is cut there.",
        advanced=True,
    )


@effect(category="glitch", params=SignalNoiseParams)
def signal_noise(rgb: np.ndarray, params: SignalNoiseParams) -> np.ndarray:
    """Mix random grey static into `bands` full-width bands, then stamp `dropouts` short streaks of `color` on top.

    A band of static replaces `static` of every pixel in it by one random grey (the same in all three channels, a
    different one for each pixel). A dropout is a short streak `dropout_height` rows tall in one flat colour. Bands
    and streaks draw from separate random streams of `seed`, so changing the number of bands moves no streak.
    """
    height, width = rgb.shape[:2]
    out = rgb.copy()
    band_rng, dropout_rng = (np.random.default_rng([params.seed, stream]) for stream in range(2))
    for _ in range(params.bands):
        thickness = _extent(band_rng, params.band_height, height)
        top = int(band_rng.integers(height - thickness + 1))
        grey = band_rng.random((thickness, width), dtype=np.float32)[..., None]
        out[top : top + thickness] = out[top : top + thickness] * (1 - params.static) + grey * params.static
    for _ in range(params.dropouts):
        length = _extent(dropout_rng, params.dropout_length, width)
        row = int(dropout_rng.integers(height))
        start = int(dropout_rng.integers(width - length + 1))
        out[row : row + params.dropout_height, start : start + length] = params.color
    return out


def _extent(rng: np.random.Generator, longest: float, size: int) -> int:
    """A length in px drawn between half of `longest` and `longest` (fractions of `size`), from 1 px to `size`."""
    return min(size, max(1, round(rng.uniform(longest / 2, longest) * size)))
