"""The `light_leak` effect: warm, seeded washes of light bleeding in from one side, as from a film leak."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.light import screen


class LightLeakParams(EffectParams):
    """A light leak: `blobs` soft patches of light placed at random near one side of the picture, tinted `color` and
    screened over it."""

    color: Color = param(
        "#ff8a3d",
        doc="The tint of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the orange of a leaking "
        "film back; red or yellow give other leaks.",
    )
    amount: float = param(
        0.8,
        unit="ratio",
        min=0,
        max=2,
        doc="How strongly the light is screened over the picture: 0 = off, 1 = the brightest patch at full "
        "brightness in `color`; above 1 it saturates to white sooner.",
    )
    size: float = param(
        0.25,
        unit="fraction",
        min=0.01,
        max=1,
        doc="How big the patches are and how deep they reach into the picture, as a fraction of its diagonal: "
        "each patch is a soft bump whose width is between half of this and all of it, centred between the edge "
        "and this far in. Raising it floods more of the picture.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        doc="The side the light comes in from, as the direction from the picture's centre, counterclockwise as seen "
        "on it: 0 = the right edge, 90 = the top, 180 = the left, 270 = the bottom; in between, the corner.",
    )
    blobs: int = param(
        3,
        unit="count",
        min=1,
        max=16,
        doc="How many patches of light there are, at random places along that side (each brighter or fainter and "
        "stretched along the edge by chance). Overlapping patches merge into one wash; raising it gives a more "
        "even, wider leak.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same leak (the same patches in the same places), another "
        "seed a different one.",
    )


@effect(category="light", params=LightLeakParams)
def light_leak(rgb: np.ndarray, params: LightLeakParams) -> np.ndarray:
    """Warm washes of light coming in from the side `angle` points to: `blobs` seeded patches, tinted `color` and
    screened over the picture by `amount`.

    Each patch is a Gaussian bump, centred a random depth (up to `size` x the diagonal) in from that side and a random
    place along it, with a random width of half to all of `size` x the diagonal, stretched along the edge by up to
    twice and weighted by half to all. The patches add up and are squashed smoothly into 0..1 (1 - exp(-sum)), so
    overlaps never clip into flat plateaus. The picture only ever gets brighter.
    """
    if params.amount == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    reach = params.size * float(np.hypot(height, width))
    theta = np.radians(params.angle)
    cos, sin = float(np.cos(theta)), float(np.sin(theta))  # Python floats, so the maps below stay float32
    down, right = np.mgrid[0:height, 0:width].astype(np.float32)
    right -= (width - 1) / 2
    up = (height - 1) / 2 - down
    toward = right * cos + up * sin  # px toward the side the light comes from
    along_edge = up * cos - right * sin  # px along that side, counterclockwise
    # how far the side lies from the centre and how far the picture reaches along it, at this angle
    half_depth = (width * abs(cos) + height * abs(sin)) / 2
    half_edge = (width * abs(sin) + height * abs(cos)) / 2
    depth = half_depth - toward  # px in from the side
    rng = np.random.default_rng(params.seed)
    draws = zip(*(rng.random(params.blobs) for _ in range(5)), strict=True)  # constant: five random numbers per patch
    wash = np.zeros((height, width), np.float32)
    for place, offset, spread, stretch, weight in draws:
        sigma = reach * (0.5 + 0.5 * float(spread))
        across = (along_edge - (2 * float(offset) - 1) * half_edge) / (sigma * (1 + float(stretch)))
        wash += (0.5 + 0.5 * float(weight)) * np.exp(-0.5 * (((depth - float(place) * reach) / sigma) ** 2 + across**2))
    light = (1.0 - np.exp(-wash)).astype(np.float32)
    tint = np.asarray(params.color, np.float32)
    return screen(rgb, params.amount * light[..., None] * tint)
