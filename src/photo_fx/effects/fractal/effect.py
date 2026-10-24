"""The `fractal` effect: the Mandelbrot set or a Julia set, coloured by smooth escape time, in place of the picture."""

from typing import Literal

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.ramp import ramp

# constant: the escape radius. Any radius above 2 decides escape correctly; a large one makes the smooth count
# n + 1 - log2(log|z| / log R) continuous across iteration boundaries (Linas Vepstas, "Renormalizing the Mandelbrot
# Escape").
_ESCAPE_RADIUS = 256.0
# constant: 4 is the diameter of the disc |z| <= 2, which holds the Mandelbrot set and every Julia set; zoom 1 fits it.
_VIEW_SPAN = 4.0


class FractalParams(EffectParams):
    """An escape-time fractal: the plane's points z (or c) iterated through z -> z^2 + c until they run away."""

    kind: Literal["mandelbrot", "julia"] = param(
        "julia",
        doc="Which set: 'mandelbrot' (every point is the constant c, z starts at 0; centre it with x = -0.5 to see "
        "the whole set) or 'julia' (every point is the start z, with the one constant `c_real` + i `c_imag`).",
    )
    zoom: float = param(
        1.0,
        unit="ratio",
        min=0.1,
        max=10000,
        doc="Magnification. At 1 the picture's shorter side shows 4 units of the complex plane, enough for the whole "
        "Mandelbrot set or any Julia set; 2 shows 2 units.",
    )
    x: float = param(
        0.0,
        unit="ratio",
        min=-2,
        max=2,
        doc="The real part of the point at the centre of the picture, in units of the complex plane. 0 centres a Julia "
        "set; -0.5 centres the Mandelbrot set.",
    )
    y: float = param(
        0.0,
        unit="ratio",
        min=-2,
        max=2,
        doc="The imaginary part of the point at the centre of the picture, in units of the complex plane; positive "
        "moves the view up.",
    )
    c_real: float = param(
        -0.8,
        unit="ratio",
        min=-2,
        max=2,
        doc="Julia only: the real part of the constant c in z -> z^2 + c. It decides the shape: a point inside the "
        "Mandelbrot set gives a connected Julia set, one outside a dust. Ignored by 'mandelbrot'.",
    )
    c_imag: float = param(
        0.156,
        unit="ratio",
        min=-2,
        max=2,
        doc="Julia only: the imaginary part of the constant c. Ignored by 'mandelbrot'.",
    )
    iterations: int = param(
        64,
        unit="count",
        min=1,
        max=2000,
        doc="How many times z -> z^2 + c is applied before a point is taken to stay for ever (the interior). More "
        "reveals finer detail at the set's edge and costs time in proportion; zooming in needs more.",
    )
    period: float = param(
        24.0,
        unit="count",
        min=1,
        max=1000,
        doc="How many iterations of escape time take the colours from the first to the last of `colors`; they then "
        "run back, so the bands cycle without a seam. Smaller = more, thinner bands.",
        advanced=True,
    )
    colors: tuple[Color, ...] = param(
        ("#12002b", "#6a1bff", "#ff2bd6", "#ffd84a"),
        doc="The colours of the bands, evenly spaced; at least two. Points that escape early (far from the set) take "
        "the first colours.",
        min_length=2,
    )
    interior: Color = param(
        "#000000",
        doc="The colour of the points that never escape: the inside of the set.",
        advanced=True,
    )


@effect(category="backgrounds", params=FractalParams)
def fractal(rgb: np.ndarray, params: FractalParams) -> np.ndarray:
    """Replace the picture with the Mandelbrot set or a Julia set, coloured by how fast each point escapes.

    A generated background: the input is used only for its height and width, its content is ignored. A point
    escapes when |z| exceeds 256 after n steps of z -> z^2 + c; its colour is then `colors` at the smooth escape count
    n + 1 - log2(log|z| / log 256) over `period` (a continuous count, so no steps between bands), and a point that has
    not escaped after `iterations` steps takes `interior`. Only the unescaped points are iterated, in single
    precision.
    """
    height, width = rgb.shape[:2]
    step = _VIEW_SPAN / params.zoom / min(height, width)  # plane units per pixel
    real = params.x + (np.arange(width) - (width - 1) / 2) * step
    imag = params.y - (np.arange(height) - (height - 1) / 2) * step  # rows run downwards, the imaginary axis up
    point = (real[None, :] + 1j * imag[:, None]).astype(np.complex64).ravel()
    if params.kind == "julia":
        z, c = point, np.full_like(point, complex(params.c_real, params.c_imag))
    else:
        z, c = np.zeros_like(point), point
    count = np.zeros(point.size, np.float32)  # smooth escape count; stays 0 for points that never escape
    interior = np.ones(point.size, bool)
    alive = np.arange(point.size)  # the points that have not escaped yet: their positions, orbits and constants
    for n in range(1, params.iterations + 1):
        z = z * z + c
        squared = z.real**2 + z.imag**2
        gone = squared > _ESCAPE_RADIUS**2
        if gone.any():
            log_radius = 0.5 * np.log(squared[gone])  # log |z|
            count[alive[gone]] = n + 1 - np.log2(log_radius / np.log(_ESCAPE_RADIUS))
            interior[alive[gone]] = False
            stay = ~gone
            alive, z, c = alive[stay], z[stay], c[stay]
    position = 1 - np.abs(1 - (count / params.period) % 2)  # forward and back along the colours
    out = np.where(interior[:, None], np.array(params.interior, np.float32), ramp(position, params.colors))
    return out.reshape(height, width, 3).astype(np.float32)
