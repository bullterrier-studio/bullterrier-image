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
        60.0,
        unit="px",
        min=0.5,
        max=600,
        doc="How far each spike reaches, in px tuned for a picture about 480 px high: it fades linearly from the "
        "point to nothing at this distance. Raising it makes longer, fainter-tipped spikes.",
    )
    width: float = param(
        1.0,
        unit="px",
        min=1,
        max=20,
        advanced=True,
        doc="The thickness in px of each spike, tuned for a picture about 480 px high, with a soft edge one px wide: "
        "1 = a hairline. Raising it makes bolder spikes.",
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
        doc="The direction of the first spike, in degrees clockwise from pointing right, as seen on screen: 0 = to the "
        "right, 90 = down. The other spikes follow at equal angles, so with 4 spikes 45 turns the cross into an X.",
    )
    spacing: float = param(
        0.0,
        unit="px",
        min=0,
        max=200,
        doc="The average distance in px, tuned for a picture about 480 px high, between the points that send out "
        "spikes inside a bright area: each bright pixel sends them with the chance 1 / spacing^2 (the same ones for "
        "the same `seed`; raising it only drops points), so the glints keep their number per area of the picture "
        "at any size. 0, or anything below 1, = every bright point. Raise it to turn a bright area into separate "
        "glints; 13 gives a scatter of glitter.",
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
        doc="Random seed for `spacing` and `variation`: the same seed picks the same points of the same picture, "
        "another seed others.",
    )
    color: Color = param(
        "#ffffff",
        doc="The tint of the spikes: '#rrggbb', '#rgb' or [r, g, b] 0..1. White keeps the colour of the light that "
        "makes them.",
    )


@effect(category="light", params=StarburstParams)
def starburst(rgb: np.ndarray, params: StarburstParams) -> np.ndarray:
    """Diffraction spikes, sparkle or star glints: what is brighter than `threshold` gets `rays` thin lines of light,
    tinted `color` and screened over the picture by `amount`. `limit` caps how many pixels count as bright, `spacing`
    thins them to a random few, `variation` dims each by a random amount (both from `seed`).

    Each spike is `width` px wide, starts at the point at full brightness and fades linearly to nothing at `length` px;
    they leave the point at equal angles, the first at `angle`. Each point's light is scaled by 0 or 1 (`spacing`)
    and by a random number from 1 - `variation` to 1; the numbers depend on `seed` and the picture's size only, so a
    point keeps its draw while the picture around it changes. Light is only drawn from points inside the picture, so
    nothing enters from beyond its edge. The picture only ever gets brighter, and black stays black.
    """
    if params.amount == 0 or params.limit == 0:
        return rgb.copy()
    half = int(np.ceil(max(params.length, params.width)))  # the kernel covers the spike's length and its thickness
    # offsets (row, column) from the point, row growing downward and the angle clockwise on the picture
    down, right = np.mgrid[-half : half + 1, -half : half + 1].astype(np.float64)
    kernel = np.zeros(down.shape)
    for spike in range(params.rays):
        theta = np.radians(params.angle) + 2 * np.pi * spike / params.rays
        along = right * np.cos(theta) + down * np.sin(theta)  # px out along the spike's direction
        across = np.abs(down * np.cos(theta) - right * np.sin(theta))  # px sideways from its line
        thin = np.clip(params.width / 2 + 0.5 - across, 0.0, 1.0)  # a line `width` px wide, soft at its sides
        fade = np.clip(1.0 - along / params.length, 0.0, 1.0) * (along >= 0)
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
    chosen = rng.random(bright.shape[:2]) < 1 / max(params.spacing, 1) ** 2  # a chance of 1 below a spacing of 1
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
