"""The `flow_warp` effect: the picture carried along a field of noise-driven currents, smearing into streaks."""

from typing import Literal

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal
from photo_fx.ops.warp import remap


class FlowWarpParams(EffectParams):
    """A flow warp: the picture carried `amount` px along currents whose direction is set by fractal noise."""

    amount: float = param(
        20.0,
        unit="px",
        min=0,
        max=500,
        doc="How far in pixels the picture is carried along the flow in all, the length of the path each pixel's "
        "content travels in `steps` equal steps (the path bends with the flow, so it ends nearer than this where it "
        "turns). Raising it makes longer streaks; 0 leaves the picture unchanged.",
    )
    steps: int = param(
        6,
        unit="count",
        min=1,
        max=32,
        doc="How many equal steps the journey is made in, the picture being resampled once per step: with 1 every "
        "pixel moves straight `amount` px in the direction of the flow where it stands; more steps follow the "
        "bends of the flow and smear the picture into streaks along it, a little softer each time. Raising it costs "
        "time in proportion.",
    )
    scale: float = param(
        80.0,
        unit="px",
        min=1,
        max=1024,
        doc="Size of the coarsest swirls of the flow in pixels, the spacing of the first noise lattice. Larger = "
        "broad, sweeping currents; small = fine, busy eddies.",
    )
    octaves: int = param(
        3,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed to make the flow, each finer than the last (by `lacunarity`) and "
        "weaker (by `persistence`). 1 = smooth currents, more = more fine, ragged detail. Layers finer than one pixel "
        "are dropped.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer is than the one before: each layer's weight is this to the power of its "
        "number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = "
        "every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same flow, another seed a different one.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="The direction the flow runs in on average, in degrees clockwise from pointing right: 0 carries the "
        "picture to the right, 90 down, -90 up, 180 to the left. With `variation` 0 the whole picture is carried this "
        "way.",
    )
    variation: float = param(
        60.0,
        unit="degrees",
        min=0,
        max=360,
        doc="How far the noise turns the flow away from `angle`: the typical turn, in degrees, either way (the "
        "noise's standard deviation). 0 = one straight current; 60 = streaks that wander about the angle; 360 = "
        "currents running every way, with eddies where they meet.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a pixel near the border sees when the flow carries it in from beyond the picture: 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
    )


@effect(category="distortion", params=FlowWarpParams)
def flow_warp(rgb: np.ndarray, params: FlowWarpParams) -> np.ndarray:
    """Carry the picture `amount` px along a flow whose direction is `angle` plus `variation` degrees of fractal noise.

    The flow at each pixel points `angle` degrees clockwise from pointing right, turned by the noise field (scaled
    to `variation` degrees of spread). The picture is moved along it in `steps` equal steps of `amount` / `steps` px,
    each step moving what is at a pixel one step along the flow there, so a pixel's content follows the bends of the
    flow and piles up into streaks where currents meet. The path of any content is `amount` px long.
    """
    height, width = rgb.shape[:2]
    noise = fractal(
        (height, width),
        scale=params.scale,
        octaves=params.octaves,
        persistence=params.persistence,
        lacunarity=params.lacunarity,
        seed=params.seed,
    )
    direction = np.radians(params.angle) + np.radians(params.variation) * noise  # clockwise from pointing right
    step = params.amount / params.steps
    rows = np.arange(height)[:, None] - step * np.sin(direction)  # a pixel shows what was a step behind it
    cols = np.arange(width)[None, :] - step * np.cos(direction)
    carried = rgb
    for _ in range(params.steps):
        carried = remap(carried, rows, cols, interpolation=params.interpolation, edge=params.edge)
    return carried
