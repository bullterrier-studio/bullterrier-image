"""The `gradient_map` effect: brightness mapped onto a gradient of colours (duotone, tritone...)."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.ramp import ramp

# constant: only keeps a flat picture (every pixel equally bright) from dividing by zero; it maps to the first stop.
_FLAT = 1e-6


class GradientMapParams(EffectParams):
    """A gradient that brightness is mapped onto, after an automatic contrast stretch."""

    stops: tuple[Color, ...] = param(
        ("#000000", "#ffffff"),
        doc="The gradient's colours from darkest to brightest, evenly spaced; at least two. Two make a duotone, three "
        "a tritone.",
        min_length=2,
    )
    shadow_clip: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=0.5,
        doc="Share of the darkest pixels (0..0.5) that the contrast stretch pushes onto the first stop; 0 maps only "
        "the darkest pixel there. Larger gives deeper shadows.",
    )
    highlight_clip: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=0.5,
        doc="Share of the brightest pixels (0..0.5) that the contrast stretch pushes onto the last stop. Larger gives "
        "brighter highlights.",
    )
    quantile_method: Literal[
        "linear",
        "lower",
        "higher",
        "nearest",
        "midpoint",
        "inverted_cdf",
        "averaged_inverted_cdf",
        "closest_observation",
        "interpolated_inverted_cdf",
        "hazen",
        "weibull",
        "median_unbiased",
        "normal_unbiased",
    ] = param(
        "linear",
        doc="How the stretch's clip points are read from the picture's brightness (numpy's quantile methods): 'linear' "
        "interpolates between the two nearest pixels; 'lower', 'higher', 'nearest' and 'midpoint' take the darker, "
        "the brighter, the nearer pixel or their mean; 'inverted_cdf', 'averaged_inverted_cdf', "
        "'closest_observation', 'interpolated_inverted_cdf', 'hazen', 'weibull', 'median_unbiased' and "
        "'normal_unbiased' are the textbook sample-quantile definitions (Hyndman and Fan). Only visible on small or "
        "flat pictures.",
    )


@effect(category="colour", params=GradientMapParams)
def gradient_map(rgb: np.ndarray, params: GradientMapParams) -> np.ndarray:
    """Recolour by brightness: darkest pixels get the first stop, brightest the last, evenly spaced between.

    Two stops make a duotone, three a tritone; e.g. stops = ["#0b0020", "#ff2bd6", "#00f0ff"] is a neon scheme.
    The picture is first stretched to use its full tonal range (cutting `shadow_clip` and `highlight_clip` off its
    ends), so the whole gradient is always visible.
    """
    lum = luminance(rgb)
    lo, hi = np.quantile(lum, [params.shadow_clip, 1.0 - params.highlight_clip], method=params.quantile_method)
    return ramp(np.clip((lum - lo) / max(hi - lo, _FLAT), 0.0, 1.0), params.stops)
