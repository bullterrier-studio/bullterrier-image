"""Relief: a height field's slopes."""

import numpy as np

from bullterrier_image.ops.blur import NUMPY_PAD
from bullterrier_image.ops.convolve import Edge


def slopes(height: np.ndarray, *, scale: float, edge: Edge) -> tuple[np.ndarray, np.ndarray]:
    """How fast the surface `height` (height, width) rises per pixel down the picture and across it, as `(rise_y,
    rise_x)`, each of the input's shape.

    A slope is the central difference of the two neighbours, `(next - previous) / 2`, times `scale`, the height of a
    value of 1 in px. `edge` is what lies beyond the picture's edge, as in `bullterrier_image.ops.convolve`: 'reflect'
    (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or
    'wrap' (the opposite edge); it decides the slope in the first and last row and column only.
    """
    padded = np.pad(height, 1, mode=NUMPY_PAD[edge])
    rise_y = scale * (padded[2:, 1:-1] - padded[:-2, 1:-1]) / 2  # a central difference spans 2 px
    rise_x = scale * (padded[1:-1, 2:] - padded[1:-1, :-2]) / 2  # a central difference spans 2 px
    return rise_y, rise_x
