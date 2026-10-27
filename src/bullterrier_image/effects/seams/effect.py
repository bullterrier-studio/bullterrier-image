"""The `seams` effect: a solid line or band along every border between the pieces of a partition."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.pieces import border_distance
from bullterrier_image.ops.voronoi import line_coverage


class SeamsParams(EffectParams):
    """The settings of `seams`: how wide the line is and what colour."""

    width: float = param(
        2.0,
        unit="px",
        min=0,
        max=48,
        doc="Width of the line in px at the recipe's size, centred on each border: 0 draws nothing, 1 is a hairline, "
        "10 or more a band. Raising it widens the line.",
    )
    color: Color = param(
        "#ffffff",
        doc="Colour of the line, solid: '#rrggbb', '#rgb' or three numbers from 0 to 1.",
    )


@effect(category="stylised", params=SeamsParams)
def seams(rgb: np.ndarray, params: SeamsParams, *, partition: np.ndarray | None = None) -> np.ndarray:
    """Draw a solid line or band along every border between the pieces of a partition.

    The line is one flat colour, centred on each border and anti-aliased along its edges; the picture's own border is
    not a border. The pieces come from the step's `partition` key, which names a region of the recipe: a partition
    (grid, shards, cuts, svg or bands) or a selection such as the subject (counted as two pieces, what it chooses
    and what it does not), which outlines its hard edge. Without a partition (or with a single piece) there is no
    border and the picture is unchanged.

    `partition` is filled in by the recipe (never set it in a recipe file): int32 labels, one piece number per pixel.
    """
    if partition is None:
        return rgb.copy()
    cover = line_coverage(border_distance(partition), params.width).astype(np.float32)[..., None]
    return (rgb + (np.asarray(params.color, np.float32) - rgb) * cover).astype(np.float32)
