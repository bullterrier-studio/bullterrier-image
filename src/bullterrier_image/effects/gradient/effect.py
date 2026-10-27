"""The `gradient` effect: a colour gradient (linear, radial or conic), or one flat colour, in place of the picture."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.ramp import ramp


class GradientParams(EffectParams):
    """A gradient through a list of colours, laid out as a straight band, rings round a centre or a sweep round it."""

    kind: Literal["linear", "radial", "conic"] = param(
        "linear",
        doc="The layout: 'linear' (colours run in a straight band across the whole picture, at `angle`), 'radial' "
        "(rings round the centre `x`, `y`, the last colour `radius` away) or 'conic' (a clockwise sweep round the "
        "centre, starting at `angle`; the last colour meets the first at a hard seam).",
    )
    colors: tuple[Color, ...] = param(
        ("#12002b", "#ff2bd6"),
        doc="The gradient's colours in order, evenly spaced; at least one. Linear: from the start of the band to its "
        "end. Radial: from the centre outwards. Conic: round the turn. One colour is a flat fill of the whole "
        "picture, and every other parameter is then ignored.",
        min_length=1,
    )
    angle: float = param(
        90.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="Linear: the direction the colours run, measured clockwise on screen from the right: 0 = left to right, 90 "
        "= top to bottom. Conic: where the sweep starts, on the same scale (0 = right of the centre, 90 = below it). "
        "Ignored by 'radial'.",
    )
    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Horizontal position of the centre as a fraction of the width (0 = left edge, 1 = right edge). Used by "
        "'radial' and 'conic'; 'linear' is always centred.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Vertical position of the centre as a fraction of the height (0 = top edge, 1 = bottom edge). Used by "
        "'radial' and 'conic'; 'linear' is always centred.",
    )
    radius: float = param(
        0.75,
        unit="fraction",
        min=0.01,
        max=4,
        doc="Radial only: the distance from the centre at which the last colour is reached, as a fraction of the "
        "picture's shorter side; farther out stays on the last colour. Larger = a wider, softer glow. Ignored by "
        "'linear' and 'conic'.",
    )
    repeat: int = param(
        1,
        unit="count",
        min=1,
        max=64,
        doc="How many times the list of colours is fitted along the gradient (the band, the radius, one turn): "
        "1 = once, 2 = twice as fast. Radial stays on the last colour beyond the radius.",
    )
    reflect: bool = param(
        False,
        doc="Run every second repeat backwards, so neighbouring repeats meet on the same colour instead of jumping "
        "from the last colour back to the first. Has no effect with `repeat` 1.",
    )


@effect(category="backgrounds", params=GradientParams)
def gradient(rgb: np.ndarray, params: GradientParams) -> np.ndarray:
    """Replace the picture with a gradient (linear, radial or conic), or with a flat colour fill (preset `fill`: black;
    set colors for another); to replace the background: mask = "background".

    A generated background: the input is used only for its height and width, its content is ignored. Sizes in
    fractions of the picture, so the look does not depend on its resolution. For example `colors` = ["#12002b",
    "#ff2bd6"] with `kind` = "radial" is a magenta glow on deep violet. A single colour is a flat fill.
    """
    height, width = rgb.shape[:2]
    angle = np.radians(np.float32(params.angle))
    centred = params.kind == "linear"  # a band has no centre of its own: it passes through the picture's middle
    cx = 0.5 * width if centred else params.x * width
    cy = 0.5 * height if centred else params.y * height
    # pixel centres relative to that point, in px: dy is a column (h, 1), dx a row (1, w)
    dy = (np.arange(height, dtype=np.float32) + 0.5 - cy)[:, None]
    dx = (np.arange(width, dtype=np.float32) + 0.5 - cx)[None, :]
    if params.kind == "linear":
        # the band is as long as the picture's extent along the direction, so its ends touch the corners (as in CSS)
        extent = abs(width * np.cos(angle)) + abs(height * np.sin(angle))
        t = (dx * np.cos(angle) + dy * np.sin(angle)) / extent + 0.5
    elif params.kind == "radial":
        t = np.hypot(dx, dy) / (params.radius * min(height, width))
    else:
        t = ((np.arctan2(dy, dx) - angle) / (2 * np.pi)) % 1.0
    cycles = np.clip(t, 0.0, 1.0) * params.repeat
    run = np.minimum(np.floor(cycles), params.repeat - 1)  # which repeat; the very end belongs to the last one
    within = cycles - run
    if params.reflect:
        within = np.where(run % 2 == 1, 1 - within, within)
    return ramp(within.astype(np.float32), params.colors)
