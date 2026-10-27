"""The `dead_pixels` effect: a screen's faulty pixels, some stuck on a bright colour and some dead black."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect


class DeadPixelsParams(EffectParams):
    """Dead pixels: `count` cells of the picture, each `size` px square, are faulty. A share `stuck` of them is stuck on
    `color` and the rest are dead, black; which ones is decided by `seed`."""

    count: int = param(
        120,
        unit="count",
        min=0,
        max=100000,
        doc="How many faulty pixels there are, each a square of `size` px: 0 leaves the picture unchanged. They lie on "
        "a grid of `size` px cells, so none overlaps another, and each is a different one. A count larger than the "
        "number of cells fills every cell. Raising it adds faulty pixels and never moves one already there. "
        "A count, not a density: the pixels' size is in design px, so the same count covers the same share of any "
        "picture size.",
    )
    size: int = param(
        4,
        unit="px",
        min=1,
        max=64,
        doc="The width and height in px of one faulty pixel, tuned for a picture about 480 px high: 1 is a single "
        "pixel of the picture, larger is a screen with bigger pixels, a block at the edge being cut off by it.",
    )
    stuck: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The share of the faulty pixels that are stuck on `color`; the rest are dead and black. 0 = all dead, "
        "1 = all stuck. The number stuck is `count` x `stuck`, rounded to a whole pixel.",
    )
    color: Color = param(
        "#ffffff",
        doc="The colour a stuck pixel shows whatever the picture is under it: '#rrggbb', '#rgb' or [r, g, b] 0..1. "
        "White is the usual fault; a red, green or blue is one stuck sub-pixel.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Chooses where the faulty pixels are and which of them are stuck; the same seed and count give the same "
        "picture, another seed another one.",
    )


@effect(category="glitch", params=DeadPixelsParams)
def dead_pixels(rgb: np.ndarray, params: DeadPixelsParams) -> np.ndarray:
    """Make `count` cells of `size` px faulty: a share `stuck` of them shows `color`, the rest black.

    The picture is divided into cells of `size` px (the last row and column of cells may be cut off by the picture's
    edge). `seed` shuffles the cells and the first `count` are the faulty ones, so a larger count keeps every pixel a
    smaller one had; the first `round(count x stuck)` of those are stuck. Everything else is the picture, bit for bit.
    """
    height, width = rgb.shape[:2]
    rows, cols = -(-height // params.size), -(-width // params.size)  # cells, the last ones cut off by the edge
    faulty = np.random.default_rng(params.seed).permutation(rows * cols)[: params.count]
    stuck = int(np.round(len(faulty) * params.stuck))
    stuck_cells, dead_cells = np.zeros(rows * cols, bool), np.zeros(rows * cols, bool)
    stuck_cells[faulty[:stuck]] = True
    dead_cells[faulty[stuck:]] = True

    def pixels(cells: np.ndarray) -> np.ndarray:
        """The pixels of the picture that lie in the marked cells."""
        grid = np.repeat(np.repeat(cells.reshape(rows, cols), params.size, axis=0), params.size, axis=1)
        return grid[:height, :width]

    out = rgb.copy()
    out[pixels(stuck_cells)] = np.asarray(params.color, np.float32)
    out[pixels(dead_cells)] = 0.0  # constant: a dead pixel emits no light
    return out
