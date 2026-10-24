"""The `plasma` effect: a warped sum of sine waves, coloured through a (by default cyclic) ramp."""

import math

import numpy as np
from scipy.special import ndtr

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import standardise
from photo_fx.ops.ramp import ramp


def _wave(
    rng: np.random.Generator,
    x: np.ndarray,
    y: np.ndarray,
    wavelength: float,
    push_x: np.ndarray | float,
    push_y: np.ndarray | float,
) -> np.ndarray:
    """One plane sine wave of `wavelength` px in a random direction and phase, read at the positions x and y (a row and
    a column of pixel places) pushed along by `push_x` and `push_y` px."""
    turn = 2 * math.pi  # constant: a full turn in radians, the range of a random direction or phase
    angle, phase = turn * rng.random(), turn * rng.random()
    return np.sin(turn / wavelength * ((x + push_x) * math.cos(angle) + (y + push_y) * math.sin(angle)) + phase)


class PlasmaParams(EffectParams):
    """A plasma: layers of crossed sine waves, each layer pushed about by the one before, shown through `colors`."""

    scale: float = param(
        160.0,
        unit="px",
        min=4,
        max=1024,
        doc="Wavelength in pixels of the coarsest layer's waves, the distance from crest to crest; tuned for a "
        "picture about 480 px high. Larger = broad, slow swirls; small = fine, busy ripples.",
    )
    octaves: int = param(
        2,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of waves are summed, each finer than the last (by `lacunarity`) and weaker (by "
        "`persistence`). Each layer is two sine waves crossing at random angles. 1 = a plain weave of two waves, more "
        "= richer, busier patterns. Layers with fewer than 2 px a wavelength are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before: each layer's weight is this to the power of its "
        "number. 0 = only the coarsest layer, 1 = every layer equally strong (very busy).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: its wavelength is divided by this per layer. 1 = every "
        "layer the same size, 2 = each half as long, larger = detail drops off faster in size.",
    )
    warp: float = param(
        25.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels each layer's waves are pushed along by the two waves of the layer before it (a layer's "
        "position is shifted by up to this much). 0 = plain straight waves; larger = the swirling, folded look of a "
        "plasma. The first layer has nothing before it, so it is not warped.",
    )
    colors: tuple[Color, ...] = param(
        ("#ff0080", "#ff8c00", "#ffee00", "#00e5a0", "#00a2ff", "#7a00ff", "#ff0080"),
        doc="The colours the plasma is shown in, from its lowest values to its highest, evenly spaced along the ramp; "
        "at least two. Each pixel's place on the ramp is its rank in the pattern, so every colour is used about "
        "equally. The default's last colour is its first, so the palette comes round without a seam.",
        min_length=2,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same plasma (the angles and phases of the waves), another "
        "seed a different one.",
    )


@effect(category="backgrounds", params=PlasmaParams)
def plasma(rgb: np.ndarray, params: PlasmaParams) -> np.ndarray:
    """Replace the picture with a plasma: warped, crossed sine waves coloured through `colors`.

    A generated background: the input is used only for its height and width, its content is ignored. Layer i has two
    plane waves sin(2 pi (x cos a + y sin a) / w + p) of wavelength w = `scale` / `lacunarity`**i, random directions a
    and phases p, and weight `persistence`**i; it is evaluated at positions pushed along x and y by `warp` px times
    the two waves of layer i - 1 (so layer 0 is not warped). The sum is standardised and passed through the normal
    cumulative distribution, so a pixel's place on the colour ramp is about its rank among all pixels.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    y, x = np.arange(height, dtype=np.float32)[:, None], np.arange(width, dtype=np.float32)[None, :]
    total = np.zeros((height, width), np.float32)
    push_x: np.ndarray = np.zeros_like(x)
    push_y: np.ndarray = np.zeros_like(y)
    for octave in range(params.octaves):
        wavelength = params.scale / params.lacunarity**octave
        if wavelength < 2:  # constant: a wave needs at least 2 px a period to show on a pixel grid (Nyquist)
            break
        first = _wave(rng, x, y, wavelength, push_x, push_y)
        second = _wave(rng, x, y, wavelength, push_x, push_y)
        total += params.persistence**octave * (first + second)
        push_x, push_y = params.warp * first, params.warp * second
    return ramp(ndtr(standardise(total)).astype(np.float32), params.colors)
