"""The `quadtree` effect: flat cells that stay large where the picture is even and shrink where it has detail."""

from typing import Self

import numpy as np
from pydantic import model_validator

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.quadtree import leaves

# constant: pixels drawn at once; bounds the working memory and never changes the result
_BAND_PIXELS = 2**16


class QuadtreeParams(EffectParams):
    """The settings of `quadtree`: when a cell splits, how large and small cells get, and how they are drawn."""

    threshold: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=0.5,
        doc="How uneven a cell has to be before it splits into four: the spread of its colour, the largest standard "
        "deviation of red, green or blue inside it, on a 0 to 0.5 scale (0.5 is the most any cell can have, half black "
        "and half white). 0 splits every cell that is not perfectly flat; 0.5 splits none. Raising it gives bigger "
        "cells and a coarser picture.",
    )
    max_cell: float = param(
        60.0,
        unit="px",
        min=2,
        max=480,
        doc="The largest a cell can be, in px at the recipe's size: the picture starts as an even grid of cells at "
        "most this wide. Raising it makes the flat parts coarser.",
    )
    min_cell: float = param(
        5.0,
        unit="px",
        min=1,
        max=480,
        doc="The smallest a cell can be, in px at the recipe's size, never above `max_cell`. A cell does not split "
        "into halves smaller than this. Lowering it keeps finer detail at edges.",
    )
    gap: float = param(
        1.0,
        unit="px",
        min=0,
        max=24,
        doc="Width of the line between neighbouring cells, in px at the recipe's size: 0 = cells touch, 1 a thin "
        "grid, 5 or more visible tiles. The picture's own edge gets no line. A cell no wider than the line is all "
        "line, so keep it below `min_cell`.",
    )
    gap_color: Color = param(
        "#000000",
        doc="The colour of the lines between cells, and of the corners a `rounding` cuts away: '#rrggbb', '#rgb' or "
        "three numbers 0 to 1.",
    )
    rounding: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How round each cell's corners are, as a share of half its shorter side: 0 = square cells, 1 = a circle "
        "in each square cell.",
    )

    @model_validator(mode="after")
    def _cells_in_order(self) -> Self:
        """The cell sizes are a range: refused, naming both, when the smallest is above the largest."""
        if self.min_cell > self.max_cell:
            raise ValueError(
                f"`min_cell` ({self.min_cell}) must not exceed `max_cell` ({self.max_cell}): "
                "they are the smallest and the largest a cell can be"
            )
        return self


@effect(category="stylised", params=QuadtreeParams)
def quadtree(rgb: np.ndarray, params: QuadtreeParams) -> np.ndarray:
    """Redraw the picture as flat cells that split only where it varies: large in even areas, small at edges.

    Each cell is filled with the average colour of the photo under it. A cell splits into four while its colours spread
    more than `threshold` and its halves would still be at least `min_cell` wide; the first grid's cells are at most
    `max_cell` wide. `gap` draws a line between cells and `rounding` rounds their corners.
    """
    height, width = rgb.shape[:2]
    found = leaves(
        rgb,
        threshold=params.threshold,
        max_cell=max(1, round(params.max_cell)),
        min_cell=max(1, round(params.min_cell)),
    )
    half_gap = params.gap / 2
    line = np.asarray(params.gap_color, np.float32)
    x = (np.arange(width, dtype=np.float32) + 0.5)[None, :]
    out = np.empty_like(rgb)
    # Each pixel depends only on its own cell, so it is drawn a band of rows at a time: the same result as all at once,
    # with the temporaries of one band instead of the whole picture.
    rows = max(1, _BAND_PIXELS // width)
    for top_row in range(0, height, rows):
        band = slice(top_row, top_row + rows)
        label = found.label[band]
        y = (np.arange(label.shape[0], dtype=np.float32) + top_row + 0.5)[:, None]
        # Each pixel's cell as a rounded box, in pixel-centre coordinates. A side on the picture's edge is pushed out
        # by half the gap, so the edge gets no line.
        top, bottom, left, right = found.box.T[:, label].astype(np.float32)
        size = np.minimum(bottom - top, right - left)
        top, left = top - half_gap * (top == 0), left - half_gap * (left == 0)
        bottom, right = bottom + half_gap * (bottom == height), right + half_gap * (right == width)
        radius = params.rounding * size / 2
        past_x = np.abs(x - (left + right) / 2) - ((right - left) / 2 - radius)  # beyond the straight part of the side
        past_y = np.abs(y - (top + bottom) / 2) - ((bottom - top) / 2 - radius)
        # The signed distance to the rounded box (negative inside), the standard formula for a rounded rectangle.
        distance = (
            np.hypot(np.maximum(past_x, 0), np.maximum(past_y, 0)) + np.minimum(np.maximum(past_x, past_y), 0) - radius
        )
        # A pixel centre half a pixel inside the cell's border is fully the cell's; the line takes `half_gap` of each
        # side.
        cover = np.clip(0.5 - (distance + half_gap), 0.0, 1.0)[..., None]
        out[band] = line + (found.mean[label] - line) * cover
    return out
