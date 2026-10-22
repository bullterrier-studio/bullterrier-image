"""Finding the subject of a picture: the person, pet or object that stands out.

One question, one answer: `subject_matte(rgb)` returns how much each pixel belongs to the subject (1) or to the
background (0). Which model produces that is this module's business; callers never see it.

Today the model is Apple Vision's foreground-subject segmentation (the "lift subject" feature): macOS only, runs on the
Neural Engine, no download, and works for people, pets and objects alike. On macOS before 14 it falls back to Vision's
older people-only model. Another backend (for Linux) would be selected inside `subject_matte`.
"""

import sys

import numpy as np
from PIL import Image
from scipy import ndimage

from . import _vision


def subject_matte(rgb: np.ndarray, *, shrink: float = 1.5, feather: float = 1.0) -> np.ndarray:
    """Alpha matte (height, width, float32 0..1) for the main subject of `rgb`, same size as `rgb`.

    The segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is
    replaced. `shrink` pulls the edge in by that many pixels and `feather` blurs it by that many pixels,
    both measured in `rgb`'s own pixels; the defaults give a clean, slightly soft cut for a 1000-1600 px photo.

    Raises RuntimeError if nothing stands out, NotImplementedError off macOS.
    """
    if sys.platform != "darwin":
        raise NotImplementedError("subject_matte needs Apple Vision, which is macOS only")
    height, width = rgb.shape[:2]
    mask = _vision.subject_mask(rgb)
    if mask is None or not (mask >= 128).any():
        raise RuntimeError("no subject found in the picture (no person, pet or object stood out)")
    mask = np.asarray(Image.fromarray(mask).resize((width, height), Image.Resampling.BILINEAR))
    return refine_edge(mask.astype(np.float32) / 255.0, shrink, feather)


def refine_edge(alpha: np.ndarray, shrink: float, feather: float) -> np.ndarray:
    """Clean up the edge of a matte (height, width, 0..1): returns a new float32 matte of the same size, in 0..1.

    `shrink` (px) pulls the edge in: a pixel keeps the lowest value within a disc of that radius around it, so a halo
    up to `shrink` px wide is cut away; the picture's own border does not count as an edge. Then `feather` (px) blurs
    the edge with a Gaussian of that sigma, which softens it without changing the matte's total. 0 turns either off,
    and so does a negative value (it is not an error).
    `subject_matte` applies this to the model's answer; use it on a matte from anywhere else for the same clean cut.
    """
    if shrink > 0:
        r = int(np.ceil(shrink))
        y, x = np.ogrid[-r : r + 1, -r : r + 1]
        alpha = ndimage.grey_erosion(alpha, footprint=(x * x + y * y) <= shrink * shrink + 1e-9, mode="nearest")
    if feather > 0:
        alpha = ndimage.gaussian_filter(alpha, feather)
    return np.clip(alpha, 0.0, 1.0).astype(np.float32)
