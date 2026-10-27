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


def stretch(layer: np.ndarray, height: int, width: int) -> np.ndarray:
    """Scale a one-channel map (h, w) to exactly (height, width), bilinear: a model's answer back to the photo's size.

    The aspect ratio follows the target, not the map. Values are not clipped: each output pixel blends its input
    neighbours, so it stays within their range. float32 maps stay float; uint8 maps give their levels (0..255) as
    float32.
    """
    return np.asarray(Image.fromarray(layer).resize((width, height), Image.Resampling.BILINEAR), np.float32)


def box_resize(array: np.ndarray, height: int, width: int) -> np.ndarray:
    """Shrink an RGB image `array` to (height, width) by averaging the area each new pixel covers.

    Float32 stays float32, with no 8 bits in between. Returns the input itself when it already has that size.
    """
    if array.shape[:2] == (height, width):
        return array
    planes = [
        np.asarray(
            Image.fromarray(np.ascontiguousarray(array[..., c], np.float32)).resize(
                (width, height), Image.Resampling.BOX
            )
        )
        for c in range(array.shape[2])
    ]
    return np.dstack(planes)


def nearest_resize(array: np.ndarray, height: int, width: int) -> np.ndarray:
    """Scale `array` (h, w, ...) to (height, width) by repeating the pixel each new one falls in: edges stay hard.

    A whole-number scale gives equal blocks. Returns the input itself when it already has that size.
    """
    if array.shape[:2] == (height, width):
        return array
    rows = np.arange(height) * array.shape[0] // height
    columns = np.arange(width) * array.shape[1] // width
    return array[rows][:, columns]
