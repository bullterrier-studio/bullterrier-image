"""The `tilt_shift` effect: a sharp band across the picture, blurred and more colourful away from it, as a miniature."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.color import luminance


class TiltShiftParams(EffectParams):
    """A tilt-shift: a band of the picture stays sharp, and away from it the picture is blurred by up to `radius` and
    its colour raised by up to `saturation`."""

    focus: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the middle of the sharp band lies across the picture, as a fraction of its extent at right angles "
        "to the band: 0 = the top (for angle 0), 0.5 = the middle, 1 = the bottom.",
    )
    width: float = param(
        0.2,
        unit="fraction",
        min=0,
        max=1,
        doc="How wide the sharp band is, as a fraction of the picture's extent at right angles to it: the whole band "
        "stays exactly as it is. Raising it keeps more of the picture sharp.",
    )
    feather: float = param(
        0.25,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How far from the band's edge the blur takes to reach its full radius, as a fraction of the picture's "
        "extent at right angles to the band: the blur and the colour boost grow evenly across it. Raising it makes "
        "a gentler fall-off, lowering it an abrupt one.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        doc="The direction of the band, counterclockwise as seen on the picture: 0 = horizontal, with the focus "
        "counted from the top; 90 = vertical, counted from the left; 45 runs from the lower left to the upper right.",
    )
    radius: float = param(
        10.0,
        unit="px",
        min=0,
        max=100,
        doc="The Gaussian sigma in px at the recipe's size that the blur reaches at the full distance from the band "
        "(a further `feather` from its edge): larger blurs the far picture more; 0 = no blur.",
    )
    saturation: float = param(
        0.3,
        unit="ratio",
        min=0,
        max=2,
        doc="How much colour is added away from the band, the usual miniature look: where the blur is at its "
        "full radius each colour moves this much further (1 + saturation times) from the grey of the pixel, and "
        "partway through the feather by the same share; nothing in the band. 0 = no change in colour.",
    )
    levels: int = param(
        4,
        unit="count",
        min=1,
        max=16,
        doc="How many blurred copies of the picture the blur is built from: copy k has the sigma radius x k / "
        "levels, and each pixel mixes the two copies (or the sharp picture and the first) nearest its distance "
        "from the band. More is smoother between levels and slower; 1 mixes the sharp picture with one blur.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where each blurred copy's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is "
        "more exact and slower.",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="light", params=TiltShiftParams)
def tilt_shift(rgb: np.ndarray, params: TiltShiftParams) -> np.ndarray:
    """A miniature look: the band `width` wide at `focus` stays sharp, and away from it the picture is blurred up to
    `radius` and its colour raised by `saturation`.

    A pixel's distance from the band's middle, as a fraction of the picture's extent across it, less half the band's
    width, over `feather`, clipped to 0..1, is its blur weight w. The blur is `levels` copies of the picture with
    sigma radius x k / levels, and a pixel mixes the two copies (the sharp picture being copy 0) either side of
    w x `levels`; its colour is moved from its grey by 1 + `saturation` x w. Pixels with w = 0, the whole band, are
    returned exactly as they are.
    """
    height, width = rgb.shape[:2]
    theta = np.radians(params.angle)
    cos, sin = float(np.cos(theta)), float(np.sin(theta))  # Python floats, so the maps below stay float32
    rows = ((height - 1) / 2 - np.arange(height, dtype=np.float32))[:, None]  # px above the middle
    cols = (np.arange(width, dtype=np.float32) - (width - 1) / 2)[None, :]  # px right of the middle
    extent = height * abs(cos) + width * abs(sin)  # the picture's extent at right angles to the band, in px
    across = 0.5 - (rows * cos - cols * sin) / extent  # 0..1 across the picture, 0 at the side the focus counts from
    weight = np.clip((np.abs(across - params.focus) - params.width / 2) / params.feather, 0.0, 1.0)
    position = weight * params.levels  # 0 = sharp ... levels = the full radius
    if params.radius == 0:
        out = rgb.copy()  # every copy is the picture itself
    else:
        out = np.clip(1.0 - position, 0.0, 1.0)[..., None] * rgb  # the sharp copy's share
        for level in range(1, params.levels + 1):
            share = np.clip(1.0 - np.abs(position - level), 0.0, 1.0)
            if share.max() > 0:
                sigma = params.radius * level / params.levels
                out += share[..., None] * gaussian_blur(rgb, sigma, truncate=params.truncate, edge=params.edge)
    if params.saturation > 0:
        grey = luminance(rgb)[..., None]
        colourful = grey + (out - grey) * (1.0 + params.saturation * weight)[..., None]
        out = np.where((weight > 0)[..., None], np.clip(colourful, 0.0, 1.0), out)
    return np.clip(out, 0.0, 1.0).astype(np.float32)
