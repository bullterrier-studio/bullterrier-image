"""The `pixel_grid` effect: the picture seen on an LCD screen, each cell split into red, green and blue stripes."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class PixelGridParams(EffectParams):
    """An LCD pixel grid: square cells of `cell` px, each split into three vertical sub-pixels (red, green, blue)
    that light only their own channel, with a dark `gap` between them, mixed into the picture by `amount`, and
    optionally brightened to keep the picture's average brightness."""

    cell: int = param(
        4,
        unit="px",
        min=3,
        max=32,
        doc="Width and height in px of one grid cell, which holds one red, one green and one blue stripe, each a third "
        "of the cell wide. Larger = a coarser grid; the smallest, 3, gives stripes one pixel wide.",
    )
    gap: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="Width of the dark lines between sub-pixels, as a fraction of a stripe's width (cell / 3): the same lines "
        "run between the stripes and between rows of cells, half of each inside its own sub-pixel. 0 = stripes "
        "touch, 1 = nothing is lit and the grid is black. Raising it darkens the picture and shows the grid.",
    )
    amount: float = param(
        0.4,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the picture is shown through the grid: 0 = the picture unchanged, 1 = the picture as the grid "
        "lights it (each channel only inside its own sub-pixels, a third of the picture's brightness or less), "
        "in between a mix of the two.",
    )
    keep_brightness: bool = param(
        False,
        doc="Whether the grid is brightened by the share it darkens the picture on average, so the average "
        "brightness is kept and raising `amount` or `gap` shows the grid without dimming the picture (a lit stripe "
        "is lifted above the picture's own value and clips at 1 where that is already bright; with `gap` 1 and "
        "`amount` 1 nothing is lit and the picture is black). False = the grid only ever darkens, as an LCD does.",
    )


def _lit(count: int, period: float, start: float, stop: float) -> np.ndarray:
    """Share of each of `count` pixels (pixel i spans i to i + 1) covered by the intervals [start, stop) repeated
    every `period`, as the difference of the length lit below each pixel edge."""
    edges = np.arange(count + 1)
    below = np.floor(edges / period) * (stop - start) + np.clip(np.mod(edges, period) - start, 0, stop - start)
    return np.diff(below)


@effect(category="texture", params=PixelGridParams)
def pixel_grid(rgb: np.ndarray, params: PixelGridParams) -> np.ndarray:
    """The picture on an LCD: each `cell` px cell split into red, green and blue stripes with a dark `gap` between.

    A stripe is a third of the cell wide and lights only its own channel, in a rectangle that leaves half the gap
    free on each side (the gap's width is `gap` x the stripe's, horizontally and vertically alike). A pixel on a
    stripe's edge is lit in proportion to the area it shares with it. The result is the picture mixed with that
    pattern by `amount`, so it never brightens the picture; with `keep_brightness` the gains are divided by their mean
    over a whole cell (the same at every picture size), which brightens a lit stripe above the picture (clipped at 1).
    """
    height, width = rgb.shape[:2]
    channels = rgb.shape[-1]  # one stripe per colour channel
    stripe = params.cell / channels
    margin = params.gap * stripe / 2
    rows = _lit(height, params.cell, margin, params.cell - margin)
    columns = np.stack(
        [_lit(width, params.cell, i * stripe + margin, (i + 1) * stripe - margin) for i in range(channels)], axis=-1
    )
    lit = rows[:, None, None] * columns[None]
    gain = 1 - params.amount * (1 - lit)
    if params.keep_brightness:
        # each channel is lit over its stripe's share of a cell less the gap's border, in both directions
        lit_share = (params.cell - 2 * margin) / params.cell * (stripe - 2 * margin) / params.cell
        mean_gain = 1 - params.amount * (1 - lit_share)
        if mean_gain > 0:  # gap 1 at amount 1 lights nothing: black stays black
            gain = gain / mean_gain
    return np.clip(rgb * gain.astype(np.float32), 0.0, 1.0)  # the lifted stripes of keep_brightness can pass 1
