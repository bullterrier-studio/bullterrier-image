"""The `pixel_sort` effect: runs of pixels between two thresholds sorted by brightness or hue along rows or columns."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.colorspace import rgb_to_hsv


class PixelSortParams(EffectParams):
    """A pixel sort: along every row (or column) the pixels whose key lies between two thresholds are put in order."""

    by: Literal["brightness", "hue"] = param(
        "brightness",
        doc="What the pixels are measured and put in order by: 'brightness' (perceived brightness, Rec. 709 weights; "
        "0 = black, 1 = white) or 'hue' (the position on the colour circle, 0 = red, 1/3 = green, 2/3 = blue; a grey "
        "has hue 0). Brightness gives smooth light-to-dark smears, hue rainbow streaks.",
    )
    low: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="The lowest key, 0 to 1, a pixel may have and still be sorted: pixels below it stay where they are and "
        "break a run in two. Raising it leaves more of the dark (or low-hue) pixels alone. If it is above `high`, "
        "nothing is sorted.",
    )
    high: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="The highest key, 0 to 1, a pixel may have and still be sorted: pixels above it stay where they are and "
        "break a run in two. Lowering it leaves more of the bright (or high-hue) pixels alone, which protects the "
        "highlights. If it is below `low`, nothing is sorted.",
    )
    axis: Literal["horizontal", "vertical"] = param(
        "horizontal",
        doc="The direction the pixels are sorted along: 'horizontal' sorts along every row (the streaks run across the "
        "picture), 'vertical' along every column (the streaks run down it).",
    )
    order: Literal["ascending", "descending"] = param(
        "ascending",
        doc="Which way a run is ordered: 'ascending' puts the lowest key first (the left or top end of the run), "
        "'descending' the highest key first.",
    )


@effect(category="distortion", params=PixelSortParams)
def pixel_sort(rgb: np.ndarray, params: PixelSortParams) -> np.ndarray:
    """Sort every run of pixels whose key lies between `low` and `high` along the rows (or columns) by that key.

    A run is a stretch of consecutive pixels in one row (column) that all have a key (`by`) from `low` to `high`; a
    pixel outside that range ends the run and never moves. Each run keeps exactly the pixels it had, only put in
    `order` of their key, so every row (column) keeps its pixels and no colour is invented. Equal keys keep their
    order.
    """
    key = luminance(rgb) if params.by == "brightness" else rgb_to_hsv(rgb)[..., 0]
    across = params.axis == "horizontal"
    lines = rgb if across else rgb.transpose(1, 0, 2)  # a row of `lines` is a line to sort along
    key = key if across else key.T
    inside = (key >= params.low) & (key <= params.high)
    begins = inside.copy()
    begins[:, 1:] &= ~inside[:, :-1]  # a run begins at an inside pixel whose left neighbour is outside (or absent)
    run = np.cumsum(begins.ravel())  # runs are numbered in reading order, and a run never crosses a row
    members = np.flatnonzero(inside.ravel())
    ranked = key.ravel()[members]
    if params.order == "descending":
        ranked = -ranked
    # the members are in reading order, so each run's slots are consecutive: sorting them by (run, key) fills the same
    # slots with the same pixels in order of the key
    by_run_then_key = np.lexsort((ranked, run[members]))
    out = lines.copy()  # in reading order, so a flat index below is a pixel of `lines`
    pixels = out.reshape(inside.size, 3)
    pixels[members] = pixels[members[by_run_then_key]]
    return out if across else np.ascontiguousarray(out.transpose(1, 0, 2))
