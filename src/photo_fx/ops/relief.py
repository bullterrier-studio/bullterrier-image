"""Relief: a height field's slopes, and how much light a surface of that shape catches from a direction."""

import numpy as np

from photo_fx.ops.blur import NUMPY_PAD
from photo_fx.ops.convolve import Edge


def slopes(height: np.ndarray, *, scale: float, edge: Edge) -> tuple[np.ndarray, np.ndarray]:
    """How fast the surface `height` (height, width) rises per pixel down the picture and across it, as `(rise_y,
    rise_x)`, each of the input's shape.

    A slope is the central difference of the two neighbours, `(next - previous) / 2`, times `scale`, the height of a
    value of 1 in px. `edge` is what lies beyond the picture's edge, as in `photo_fx.ops.convolve`: 'reflect'
    (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or
    'wrap' (the opposite edge); it decides the slope in the first and last row and column only.
    """
    padded = np.pad(height, 1, mode=NUMPY_PAD[edge])
    rise_y = scale * (padded[2:, 1:-1] - padded[:-2, 1:-1]) / 2  # a central difference spans 2 px
    rise_x = scale * (padded[1:-1, 2:] - padded[1:-1, :-2]) / 2  # a central difference spans 2 px
    return rise_y, rise_x


def lambert(rise_y: np.ndarray, rise_x: np.ndarray, *, angle: float, elevation: float) -> np.ndarray:
    """The light a surface with these slopes catches (Lambert's law): the dot product of its unit normal and the
    direction towards a light, an array of the slopes' shape from -1 to 1.

    The normal is `(-rise_x, -rise_y, 1)` over its length. The light is `elevation` degrees above the picture's plane
    (0 = level with it, 90 = straight above) and comes from `angle` degrees clockwise from the top of the picture (0 =
    above, 90 = the right). A flat surface catches `sin(elevation)`; one rising away from the light catches more, one
    rising towards it less; at elevation 0 the result is how far the normal leans towards the light, 0 for a flat
    surface.
    """
    angle, elevation = np.radians(angle), np.radians(elevation)
    toward = rise_x * np.sin(angle) - rise_y * np.cos(angle)  # how fast the surface rises towards the light
    return (np.sin(elevation) - np.cos(elevation) * toward) / np.sqrt(1 + rise_x**2 + rise_y**2)
