"""The `starburst` effect: diffraction spikes drawn through the bright points of a picture."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.convolve import convolve
from bullterrier_image.ops.light import highlights, screen

# constant: the highest `threshold` there is; `limit` raises a threshold no further, so near-white pixels still light
MAX_THRESHOLD = 0.99


class StarburstParams(EffectParams):
    """A starburst: the highlights are smeared into `rays` thin spikes of light that fade with distance, and screened
    back over the picture."""

    rays: int = param(
        4,
        unit="count",
        min=1,
        max=32,
        doc="How many spikes leave each bright point, evenly spread round it: 4 = a cross, 6 = a six-pointed star, "
        "1 = a single comet tail. Raising it makes a denser star.",
    )
    length: float = param(
        0.1,
        unit="fraction",
        min=0.001,
        max=1,
        doc="How far each spike reaches, as a fraction of the picture's diagonal: it fades linearly from the point "
        "to nothing at this distance. Raising it makes longer, fainter-tipped spikes.",
    )
    threshold: float = param(
        0.95,
        unit="fraction",
        min=0,
        max=MAX_THRESHOLD,
        doc="The brightness (0..1, per channel) where a spike starts: only values above it make light, rising to "
        "full at 1. Raising it limits the spikes to the brightest points. Every pixel above it sends out spikes, so "
        "at a low threshold a large bright area (a window, a sky, white fur) floods the picture with light: raise "
        "the threshold, or cap the number of points with `limit`.",
    )
    limit: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        advanced=True,
        doc="The most pixels that send out spikes, as a share of all the picture's pixels, the brightest first: the "
        "threshold is raised just enough to keep to it (never above 0.99, so a picture with more than this share "
        "of near-white pixels still lights those). 1 = no cap, 0.001 = at most one pixel in a thousand, 0 = none. "
        "Lower it to keep a low `threshold` from flooding large bright areas.",
    )
    amount: float = param(
        0.8,
        unit="ratio",
        min=0,
        max=10,
        doc="How strongly the spikes are screened over the picture: 0 = off, 1 = a spike as bright as the point that "
        "makes it, at the point; above 1 it saturates to white sooner.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction of the first spike, counterclockwise as seen on the picture: 0 = to the right, 90 = up. "
        "The other spikes follow at equal angles, so with 4 spikes 45 turns the cross into an X.",
    )
    density: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The share of the bright points that send out spikes, picked at random (the same ones for the same "
        "`seed`; raising it only adds points): 1 = all of them, 0.01 = one in a hundred, scattered over the bright "
        "areas, 0 = none. Lower it to turn a bright area into separate glints.",
    )
    variation: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        advanced=True,
        doc="How much the points differ in brightness, and so in how far their spikes show: each one's light is "
        "scaled by a random number from 1 - `variation` to 1. 0 = all alike, 1 = from full down to nothing.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        advanced=True,
        doc="Random seed for `density` and `variation`: the same seed picks the same points of the same picture, "
        "another seed others.",
    )
    color: Color = param(
        "#ffffff",
        doc="The tint of the spikes: '#rrggbb', '#rgb' or [r, g, b] 0..1. White keeps the colour of the light that "
        "makes them.",
    )


@effect(category="light", params=StarburstParams)
def starburst(rgb: np.ndarray, params: StarburstParams) -> np.ndarray:
    """Diffraction spikes: what is brighter than `threshold` gets `rays` thin lines of light, tinted `color` and
    screened over the picture by `amount`. `limit` caps how many pixels count as bright, `density` picks a random
    share of them, `variation` dims each by a random amount (both from `seed`).

    Each spike is one px wide, starts at the point at full brightness and fades linearly to nothing at `length` x the
    picture's diagonal; they leave the point at equal angles, the first at `angle`. Each point's light is scaled by
    0 or 1 (`density`) and by a random number from 1 - `variation` to 1; the numbers depend on `seed` and the
    picture's size only, so a point keeps its draw while the picture around it changes. Light is only drawn from
    points inside the picture, so nothing enters from beyond its edge. The picture only ever gets brighter, and black
    stays black.
    """
    if params.amount == 0 or params.limit == 0:
        return rgb.copy()
    reach = params.length * float(np.hypot(*rgb.shape[:2]))  # the spike fades to nothing at this distance, in px
    half = int(np.ceil(reach))
    # offsets (row, column) from the point, row growing downward and the angle counterclockwise on the picture
    down, right = np.mgrid[-half : half + 1, -half : half + 1].astype(np.float64)
    kernel = np.zeros(down.shape)
    for spike in range(params.rays):
        theta = np.radians(params.angle) + 2 * np.pi * spike / params.rays
        along = right * np.cos(theta) - down * np.sin(theta)  # px out along the spike's direction
        across = np.abs(right * np.sin(theta) + down * np.cos(theta))  # px sideways from its line
        thin = np.clip(1.0 - across, 0.0, 1.0)  # a line one px wide, soft at its sides
        fade = np.clip(1.0 - along / reach, 0.0, 1.0) * (along >= 0)
        kernel = np.maximum(kernel, thin * fade)
    total = kernel.sum()
    kernel /= total  # `convolve` takes weights adding up to 1; the sum comes back below
    threshold = params.threshold
    if params.limit < 1:  # raise the threshold to the brightness that only `limit` of the pixels exceed
        allowed = int(params.limit * rgb.shape[0] * rgb.shape[1])
        brightest = np.partition(rgb.max(axis=-1).ravel(), -(allowed + 1))[-(allowed + 1)]
        threshold = min(max(threshold, float(brightest)), MAX_THRESHOLD)
    bright = highlights(rgb, threshold)
    rng = np.random.default_rng(params.seed)
    chosen = rng.random(bright.shape[:2]) < params.density
    scale = 1.0 - params.variation * rng.random(bright.shape[:2])
    bright *= (chosen * scale).astype(np.float32)[..., None]
    # One px of black round the picture, so that `convolve`'s 'nearest' edge extends black and not the picture's own
    # border: no light comes in from beyond the edge.
    framed = np.pad(bright, ((1, 1), (1, 1), (0, 0)))
    spikes = np.empty_like(bright)
    for channel in range(rgb.shape[-1]):  # one channel at a time keeps the transforms' working memory to a third
        spikes[..., channel] = total * convolve(framed[..., channel], kernel, edge="nearest")[1:-1, 1:-1]
    tint = np.asarray(params.color, np.float32)
    return screen(rgb, params.amount * spikes * tint)
