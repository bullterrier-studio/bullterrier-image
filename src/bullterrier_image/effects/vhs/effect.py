"""The `vhs` effect: a worn videotape or failing analog signal, with tracking bands, tape dropouts, colour bleed, noise
(static, snow) and wobble."""

from typing import Literal

import numpy as np
from scipy import ndimage

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.colorspace import rgb_to_yiq, yiq_to_rgb
from bullterrier_image.ops.noise import clumped_noise
from bullterrier_image.ops.shift import Edge, shift, slide_bands


class VhsParams(EffectParams):
    """The settings of `vhs`: tracking bands, dropouts, bleed, tape noise, wobble and the random seed."""

    tracking: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        doc="How far a tracking band slides sideways, at most, as a fraction 0 to 1 of the picture's width: each band "
        "slides by a random whole number of pixels up to this, left or right. 0 = the bands do not move.",
    )
    bands: int = param(
        3,
        unit="count",
        min=0,
        max=50,
        doc="How many horizontal tracking bands slide, 0 to 50, at random rows; 0 = none.",
    )
    dropouts: int = param(
        0,
        unit="count",
        min=0,
        max=2000,
        doc="How many dropout streaks, 0 to 2000: short white marks where the signal was lost, each as tall as a "
        "speck of tape noise (`noise_size`) and between half of `tracking` and `tracking` of the picture's width "
        "long. 0 = none; raising it covers more of the picture.",
    )
    bleed: float = param(
        5.0,
        unit="px",
        min=0,
        max=100,
        doc="How far colour bleeds to the right of what it belongs to, 0 to 100 px, tuned for a picture about 480 px "
        "high: colour lags the brightness by this many pixels and is blurred sideways by a Gaussian of this standard "
        "deviation. 0 = no bleed; raising it smears red and blue far past their edges.",
    )
    noise: float = param(
        0.03,
        unit="fraction",
        min=0,
        max=1,
        doc="The standard deviation of the tape noise added to brightness, 0 to 1 in picture units: 0 = none, 0.03 is "
        "visible, 0.1 is heavy. Colour is not touched.",
    )
    wobble: float = param(
        1.5,
        unit="px",
        min=0,
        max=50,
        doc="How far the rows wander sideways, as the standard deviation in px of a slow random drift from row to row, "
        "0 to 50, tuned for a picture about 480 px high: 0 = straight rows, a few px = the shaky edges of a worn tape.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for the bands, the dropouts, the noise and the wobble: the same seed always gives the same "
        "damage, any other a different one.",
    )
    band_height: float = param(
        0.04,
        unit="fraction",
        min=0,
        max=1,
        doc="The tallest a tracking band can be, as a fraction 0 to 1 of the picture's height; each band is between "
        "half this and this tall (never under 1 px).",
        advanced=True,
    )
    noise_size: float = param(
        1.5,
        unit="px",
        min=0.5,
        max=50,
        doc="How wide a speck of tape noise is, 0.5 to 50 px (up to 1 px every pixel is its own speck), tuned for a "
        "picture about 480 px high. Raising it gives coarser grain.",
        advanced=True,
    )
    wobble_size: float = param(
        12.0,
        unit="px",
        min=0.5,
        max=500,
        doc="How many rows one wander of the wobble spans, 0.5 to 500 px, tuned for a picture about 480 px high: "
        "small is a jitter from row to row, large a slow sway.",
        advanced=True,
    )
    truncate: float = param(
        4.0,
        unit="count",
        min=0.5,
        max=10,
        doc="Where the blurs of the bleed, the noise and the wobble are cut off, in sigmas, 0.5 to 10. Lower is "
        "faster and gives slightly sharper tails.",
        advanced=True,
    )
    edge: Literal["wrap", "nearest", "reflect", "mirror"] = param(
        "wrap",
        doc="What lies beyond the picture's left and right edges for everything that moves or blurs sideways: 'wrap' "
        "the opposite side, 'nearest' the edge pixel repeated, 'reflect' the picture mirrored (edge pixel repeated "
        "once), 'mirror' mirrored about the edge pixel. For the noise and wobble the rules act on the rows and "
        "columns of their own fields.",
        advanced=True,
    )


@effect(category="glitch", params=VhsParams)
def vhs(rgb: np.ndarray, params: VhsParams) -> np.ndarray:
    """Make the picture look like a worn videotape or a failing analog signal: tracking bands, dropouts, wobbling
    rows, bled colour and tape noise (static, snow).

    In this order: `bands` horizontal bands slide sideways by up to `tracking` of the width; every row then drifts
    sideways by a slow random wander of about `wobble` px; `dropouts` white streaks are stamped on; in YIQ (as `ntsc`)
    the colour is moved `bleed` px right and blurred sideways by that much, as colour on tape lags and smears; and
    `noise` of grain is added to the brightness. The bands, dropouts, noise and wobble each draw from their own stream
    of `seed`, so the same seed gives the same damage; changing the number of bands moves no dropout, noise or wobble.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    noise_seed = int(rng.integers(np.iinfo(np.uint32).max))
    wobble_seed = int(rng.integers(np.iinfo(np.uint32).max))
    dropout_rng = np.random.default_rng(int(rng.integers(np.iinfo(np.uint32).max)))
    bands = []
    for _ in range(params.bands):
        thickness = min(height, max(1, round(rng.uniform(params.band_height / 2, params.band_height) * height)))
        top = int(rng.integers(height - thickness + 1))
        bands.append((top, thickness, round(rng.uniform(-params.tracking, params.tracking) * width)))
    out = slide_bands(rgb, bands, edge=params.edge)
    # constant: the wander is one value for each row, a field one column wide
    wander = params.wobble * clumped_noise(
        (height, 1), size=params.wobble_size, seed=wobble_seed, truncate=params.truncate, edge=params.edge
    )
    for row in range(height):
        out[row] = _slid(out[row : row + 1], float(wander[row, 0]), params.edge)[0]
    # as tall as a speck of noise, so grain and dropouts scale together
    streak_height = max(1, round(params.noise_size))
    for _ in range(params.dropouts):
        length = min(width, max(1, round(dropout_rng.uniform(params.tracking / 2, params.tracking) * width)))
        row = int(dropout_rng.integers(height))
        start = int(dropout_rng.integers(width - length + 1))
        out[row : row + streak_height, start : start + length] = 1.0  # a dropout is white
    signal = rgb_to_yiq(out)
    sigmas = np.zeros(signal.ndim)  # a sigma of 0 leaves a direction alone: only the columns (axis 1) are blurred
    sigmas[1] = params.bleed
    chroma = _slid(signal[..., 1:], params.bleed, params.edge)
    signal[..., 1:] = ndimage.gaussian_filter(chroma, sigmas, truncate=params.truncate, mode=params.edge)
    grain = clumped_noise(
        (height, width), size=params.noise_size, seed=noise_seed, truncate=params.truncate, edge=params.edge
    )
    signal[..., 0] += params.noise * grain
    return np.clip(yiq_to_rgb(signal), 0, 1)


def _slid(rows: np.ndarray, distance: float, edge: Edge) -> np.ndarray:
    """`rows` moved `distance` px to the right, by `edge` rule, and not at all vertically."""
    return shift(rows, 0, distance, edge=edge)  # constant: no vertical distance, everything here moves sideways
