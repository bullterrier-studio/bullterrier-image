"""The `ntsc` effect: colour smeared along the scan lines and leaking into brightness, as in composite video."""

from typing import Literal

import numpy as np
from scipy import ndimage

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.colorspace import rgb_to_yiq, yiq_to_rgb


class NtscParams(EffectParams):
    """The settings of `ntsc`: how far colour smears, how much of it crawls into brightness, and where in the cycle."""

    sigma: float = param(
        4.0,
        unit="px",
        min=0,
        max=100,
        doc="How far the colour smears sideways, as the standard deviation in px of a Gaussian along each row, 0 to "
        "100, tuned for a picture about 480 px high: 0 keeps the colour where it is, raising it bleeds colour further "
        "past the edges of what it belongs to. Brightness is not smeared.",
    )
    crawl: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the colour signal leaks into brightness as dot crawl, 0 to 1: a fine diagonal chequer on "
        "coloured areas, as strong as the colour is. 0 = none, 1 = the full colour signal; grey areas show none.",
    )
    phase: float = param(
        0.0,
        unit="cycles",
        min=0,
        max=1,
        doc="Where the chequer of the dot crawl is in its cycle, 0 to 1 (1 is the same as 0): moving it sideways "
        "through a full cycle makes the dots crawl; half a cycle swaps light dots and dark dots.",
    )
    truncate: float = param(
        4.0,
        unit="count",
        min=0.5,
        max=10,
        doc="Where the smear's Gaussian is cut off, in sigmas, 0.5 to 10: the colour spreads over about this many "
        "sigmas on either side. Lower is faster and gives a slightly sharper tail.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What colour is smeared in from beyond the left and right edges: 'reflect' the picture mirrored (edge "
        "pixel repeated), 'nearest' its edge pixel repeated, 'mirror' mirrored about the edge pixel, 'wrap' the "
        "opposite side.",
        advanced=True,
    )


@effect(category="glitch", params=NtscParams)
def ntsc(rgb: np.ndarray, params: NtscParams) -> np.ndarray:
    """Smear the colour of the picture along its rows and let a share of it crawl into brightness, like an NTSC signal.

    In YIQ (FCC coefficients) the colour axes I and Q are blurred by a Gaussian along each row only, while brightness Y
    is kept. Then the colour, as a modulated subcarrier, leaks into brightness: `crawl` of I cos t + Q sin t is added
    to Y, where t = 2 pi (x / 4 + y / 2 + `phase`), because the NTSC subcarrier is sampled four times per cycle and
    flips phase on every line. Greys have no colour, so they are not touched.
    """
    height, width = rgb.shape[:2]
    signal = rgb_to_yiq(rgb)
    sigmas = np.zeros(signal.ndim)  # a sigma of 0 leaves a direction alone: only the columns (axis 1) are smeared
    sigmas[1] = params.sigma
    signal[..., 1:] = ndimage.gaussian_filter(signal[..., 1:], sigmas, truncate=params.truncate, mode=params.edge)
    # constant: NTSC samples its subcarrier 4 times per cycle (SMPTE 244M), so a pixel step is a quarter cycle
    cycles = np.arange(width) / 4 + np.arange(height)[:, None] / 2 + params.phase
    angle = 2 * np.pi * cycles
    signal[..., 0] += params.crawl * (signal[..., 1] * np.cos(angle) + signal[..., 2] * np.sin(angle))
    return np.clip(yiq_to_rgb(signal), 0, 1)
