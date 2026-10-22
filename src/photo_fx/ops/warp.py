"""Resampling through displacement fields: the one primitive behind every geometric distortion.

Promises: ``remap`` answers "what would this picture look like if each pixel were fetched from somewhere else?" with
bilinear sampling and a choice of what lies beyond the picture's edge. Fields are in pixels, relative to the pixel being
produced, so an all-zero field is the identity and a constant field is a translation.
"""

from typing import Literal

import numpy as np
from scipy import ndimage

# Our names for what lies beyond the picture, in scipy's vocabulary. scipy's plain "mirror" and "wrap" treat pixel
# centres as the period boundary (the edge pixel is not repeated), which is not what a picture edge means; the "grid-*"
# variants treat the picture as a whole tile, so "mirror" equals np.pad(mode="symmetric") and "wrap" equals np.roll.
_SCIPY_MODE = {"clamp": "nearest", "mirror": "grid-mirror", "wrap": "grid-wrap", "fill": "grid-constant"}


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
    """Resample ``image`` so that each output pixel is fetched from a displaced position.

    ``output[r, c]`` is the bilinear sample of ``image`` at ``(x = c + dx[r, c], y = r + dy[r, c])``. A positive ``dx``
    therefore pulls content from the right, which makes the picture appear to move left.

    Args:
        image: ``(h, w)`` or ``(h, w, channels)`` array of any numeric dtype, usually float32 in 0..1. A 2-D mask warps
            exactly like one channel of a colour picture. Not modified.
        dx: ``(h, w)`` horizontal displacement in pixels (fractions allowed, any magnitude, must be finite).
        dy: ``(h, w)`` vertical displacement in pixels (same rules as ``dx``).
        mode: what lies outside the picture. ``"clamp"`` repeats the nearest edge pixel, ``"mirror"`` reflects the
            picture about its edge (edge pixel repeated, like ``np.pad(mode="symmetric")``), ``"wrap"`` tiles it
            periodically (like ``np.roll``), ``"fill"`` is the constant ``fill``. Near an edge, fractional positions
            blend smoothly with whatever lies beyond it.
        fill: value outside the picture for ``mode="fill"``, in the units of ``image``; ignored by the other modes.

    Returns:
        A new float32 array with the shape of ``image``. Bilinear weights are convex, so every output value lies within
        the range of the input values (and ``fill``, in fill mode).

    Raises:
        ValueError: ``image`` is not 2-D or 3-D, ``dx``/``dy`` do not have the shape ``image.shape[:2]``, a displacement
            is NaN or infinite, or ``mode`` is not one of the four names.
    """
    if mode not in _SCIPY_MODE:
        raise ValueError(f"unknown mode {mode!r}; expected one of {', '.join(_SCIPY_MODE)}")
    image = np.asarray(image)
    if image.ndim not in (2, 3):
        raise ValueError(f"image must be (h, w) or (h, w, channels), got shape {image.shape}")
    for name, field in (("dx", dx), ("dy", dy)):
        if np.shape(field) != image.shape[:2]:
            raise ValueError(f"{name} has shape {np.shape(field)} but the image is {image.shape[:2]} (h, w)")
        if not np.isfinite(field).all():
            raise ValueError(f"{name} must be finite (found NaN or infinity)")

    # Absolute sample positions in float64: float32 cannot hold sub-pixel positions on very large pictures.
    x, y = pixel_grid(image.shape[:2])
    coords = np.stack([y + np.asarray(dy, np.float64), x + np.asarray(dx, np.float64)])

    def sample(plane: np.ndarray) -> np.ndarray:
        return ndimage.map_coordinates(
            plane, coords, order=1, mode=_SCIPY_MODE[mode], cval=fill, output=np.float32, prefilter=False
        )

    if image.ndim == 2:
        return sample(image.astype(np.float32, copy=False))
    return np.stack([sample(image[..., c].astype(np.float32)) for c in range(image.shape[2])], axis=-1)
