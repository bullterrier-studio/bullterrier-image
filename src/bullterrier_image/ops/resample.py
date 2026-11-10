"""Changing the size of a picture or a matte without leaving float precision."""

import numpy as np
from PIL import Image


def resize(array: np.ndarray, longest_side: int) -> np.ndarray:
    """Scale an RGB image, or a (height, width) alpha, so its longest side is `longest_side` px.

    Aspect ratio is kept. Works on float data without going through 8 bits, so a matte keeps its softness. Returns the
    input itself when it already has that size, otherwise a new array clipped to 0..1 (float32 in, float32 out).
    """
    height, width = array.shape[:2]
    scale = longest_side / max(height, width)
    size = (max(1, round(width * scale)), max(1, round(height * scale)))
    if size == (width, height):
        return array
    resample = Image.Resampling.LANCZOS if scale < 1 else Image.Resampling.BICUBIC
    channels = [array] if array.ndim == 2 else [array[..., c] for c in range(array.shape[2])]
    # a float32 (height, width) array is Pillow's 32-bit float mode "F" by its dtype
    scaled = [np.asarray(Image.fromarray(c.astype(np.float32, copy=False)).resize(size, resample)) for c in channels]
    out = scaled[0] if array.ndim == 2 else np.dstack(scaled)
    return np.clip(out, 0.0, 1.0)
