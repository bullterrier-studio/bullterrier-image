"""Relief: a height field's slopes, and how much light a surface of that shape catches from a direction."""

import numpy as np

from bullterrier_image.ops.blur import NUMPY_PAD, gaussian_blur
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


def lambert(rise_y: np.ndarray, rise_x: np.ndarray, *, angle: float, elevation: float) -> np.ndarray:
    """The light a surface with these slopes catches (Lambert's law): the dot product of its unit normal and the
    direction towards a light, an array of the slopes' shape from -1 to 1.

    The normal is `(-rise_x, -rise_y, 1)` over its length. The light is `elevation` degrees above the picture's plane
    (0 = level with it, 90 = straight above) and comes from `angle` degrees clockwise from pointing right, on screen
    (0 = the right, 90 = below, 180 = the left, 270 = above). A flat surface catches `sin(elevation)`; one rising away
    from the light catches more, one rising towards it less; at elevation 0 the result is how far the normal leans
    towards the light, 0 for a flat surface.
    """
    angle, elevation = np.radians(angle), np.radians(elevation)
    toward = rise_x * np.cos(angle) + rise_y * np.sin(angle)  # how fast the surface rises towards the light
    return (np.sin(elevation) - np.cos(elevation) * toward) / np.sqrt(1 + rise_x**2 + rise_y**2)


def shade(
    surface: np.ndarray, *, height: float, angle: float, elevation: float, blur: float, truncate: float, edge: Edge
) -> np.ndarray:
    """How a light from `angle` at `elevation` (as in `lambert`) changes the brightness of the relief `surface`, a
    (height, width) field of 0..1 in which 1 stands `height` px above 0: a float64 gain of the surface's shape (so a
    picture multiplied by it is rounded once), 1 where the surface is flat, above 1 on slopes facing the light, below 1
    on those facing away, 0 or less on those it cannot reach.

    The surface is first smoothed by a Gaussian of sigma `blur` px cut off at `truncate` sigmas; `edge` is what lies
    beyond the picture's edge for that blur and for the slopes, as in `slopes`. The gain is what the surface catches
    over what a flat one catches, `sin(elevation)`. The surface is left alone.
    """
    smooth = gaussian_blur(surface, blur, truncate=truncate, edge=edge)
    rise_y, rise_x = slopes(smooth, scale=height, edge=edge)
    lit = lambert(rise_y, rise_x, angle=angle, elevation=elevation)
    # a flat surface catches sin(elevation); float64 whatever the numpy version promotes the float32 slopes to
    return (lit / np.sin(np.radians(elevation))).astype(np.float64, copy=False)
