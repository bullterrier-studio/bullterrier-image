"""The `anamorphic_streaks` effect: horizontal streaks of light stretched from the highlights, as a lens draws them."""

import numpy as np
from scipy import signal

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.light import highlights, screen


class AnamorphicStreaksParams(EffectParams):
    """Anamorphic lens streaks: the highlights are stretched sideways into thin lines of light and screened back."""

    length: float = param(
        0.35,
        unit="fraction",
        min=0.001,
        max=1,
        doc="How far the streaks reach sideways, as a fraction of the picture's width: the distance over which a "
        "streak fades to 37% of its brightness (the faint tail runs on, to the picture's edge). Raising it makes "
        "longer, fainter streaks.",
    )
    threshold: float = param(
        0.7,
        unit="fraction",
        min=0,
        max=0.99,
        doc="The brightness (0..1, per channel) where a streak starts: only values above it light the streaks, "
        "rising to full at 1. Raising it limits the streaks to the brightest highlights.",
    )
    amount: float = param(
        6.0,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the streaks are screened over the picture: 0 = off; the streak is spread thin along its "
        "length, so a long one needs a high amount (above 1 it saturates to white sooner).",
    )
    color: Color = param(
        "#8fb8ff",
        doc="The tint of the streaks: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the cool blue of coated "
        "anamorphic lenses; white keeps the colour of the light that makes them.",
    )


@effect(category="light", params=AnamorphicStreaksParams)
def anamorphic_streaks(rgb: np.ndarray, params: AnamorphicStreaksParams) -> np.ndarray:
    """Horizontal streaks of light: what is brighter than `threshold` is smeared along its row, tinted `color` and
    screened over the picture by `amount`.

    The smear is a two-sided exponential, exp(-|distance| / s) with s = `length` x the picture's width, scaled to add
    up to 1: a sharp bright core with long tails, which is what a streak looks like (a Gaussian would not carry far
    enough). It runs along the rows only, so nothing leaks up or down, and no light comes in from beyond the picture's
    edge. The picture only ever gets brighter, and black stays black.
    """
    width = rgb.shape[1]
    reach = width - 1  # the farthest two columns of a row can be apart
    offsets = np.arange(-reach, reach + 1)
    kernel = np.exp(-np.abs(offsets) / (params.length * width))
    kernel /= kernel.sum()
    bright = highlights(rgb, params.threshold)
    streaks = np.empty_like(bright)
    for channel in range(rgb.shape[-1]):  # one channel at a time keeps the transforms' working memory to a third
        spread = signal.fftconvolve(bright[..., channel], kernel.astype(np.float32)[None, :], axes=1)
        streaks[..., channel] = spread[:, reach : reach + width]  # the middle `width` columns: as wide as the picture
    tint = np.asarray(params.color, np.float32)
    return screen(rgb, params.amount * streaks * tint)
