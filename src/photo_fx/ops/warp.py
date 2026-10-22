"""Resampling through displacement fields: the one primitive behind every geometric distortion.

Promises: ``remap`` answers "what would this picture look like if each pixel were fetched from somewhere else?" with
bilinear sampling and a choice of what lies beyond the picture's edge. Fields are in pixels, relative to the pixel being
produced, so an all-zero field is the identity and a constant field is a translation.
"""

from typing import Literal

import numpy as np


def pixel_grid(shape: tuple[int, int]) -> tuple[np.ndarray, np.ndarray]:
    """Pixel coordinates of a picture, the starting point for building a displacement field.

    Args:
        shape: ``(height, width)`` in pixels.

    Returns:
        ``(x, y)``, float32 arrays of shape ``(height, width)`` with ``x[r, c] = c`` and ``y[r, c] = r``
        (origin at the top-left pixel centre, y growing downwards).
    """
    h, w = shape
    x, y = np.meshgrid(np.arange(w, dtype=np.float32), np.arange(h, dtype=np.float32))
    return x, y


def remap(
    image: np.ndarray,
    dx: np.ndarray,
    dy: np.ndarray,
    *,
    mode: Literal["clamp", "mirror", "wrap", "fill"] = "clamp",
    fill: float = 0.0,
) -> np.ndarray:
    raise NotImplementedError
