"""The `pixel_grid` effect: the picture seen on an LCD screen, each cell split into red, green and blue stripes.

Its parameters are declared by the Rust struct `Params` in `crates/core/src/effects/pixel_grid.rs`.
"""

import numpy as np

from bullterrier_image._native import _core
from bullterrier_image.core.params import EffectParams
from bullterrier_image.core.registry import effect


class PixelGridParams(EffectParams, core="pixel_grid"):  # noqa: D101  (documented by its Rust declaration)
    pass


@effect(category="texture", params=PixelGridParams)
def pixel_grid(rgb: np.ndarray, params: PixelGridParams) -> np.ndarray:
    """The picture on an LCD: each `cell` px cell split into red, green and blue stripes with a dark `gap` between.

    A stripe is a third of the cell wide and lights only its own channel, in a rectangle that leaves half the gap
    free on each side (the gap's width is `gap` x the stripe's, horizontally and vertically alike). A pixel on a
    stripe's edge is lit in proportion to the area it shares with it. The result is the picture mixed with that
    pattern by `amount`, so it never brightens the picture; with `keep_brightness` the gains are divided by their mean
    over a whole cell (the same at every picture size), which brightens a lit stripe above the picture (clipped at 1).
    """
    return _core.apply("pixel_grid", rgb, params.model_dump())
