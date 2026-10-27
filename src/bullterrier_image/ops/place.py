"""Fitting a picture to a photo: how large it is, where it sits, and whether it repeats.

`place` turns any picture (an asset: a texture, a logo, a cut-out) into RGBA at the photo's size, transparent where the
picture is not, so an effect only has to combine two pictures of one size.
"""

from typing import Literal

import numpy as np
from PIL import Image

Anchor = Literal["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"]
Fit = Literal["cover", "contain", "stretch", "width", "tile"]
Resample = Literal["nearest", "bilinear", "bicubic", "lanczos"]

_FILTERS = {
    "nearest": Image.Resampling.NEAREST,
    "bilinear": Image.Resampling.BILINEAR,
    "bicubic": Image.Resampling.BICUBIC,
    "lanczos": Image.Resampling.LANCZOS,
}


def place(
    picture: np.ndarray,
    shape: tuple[int, int],
    *,
    fit: Fit,
    at: Anchor,
    scale: float,
    margin: float,
    resample: Resample,
) -> np.ndarray:
    """`picture` (float32 (h, w, 4) RGBA, straight alpha) fitted to a photo of `shape` (height, width): float32 RGBA of
    that shape, transparent wherever the picture is not.

    The picture is first sized, then multiplied by `scale`, then put in the free space of the photo inside `margin`:
    - `cover` fills the area inside the margin, keeping the aspect, and what hangs over is cropped; `contain` fits
      inside it, keeping the aspect; `stretch` fills it without keeping the aspect;
    - `width` makes the picture as wide as the photo (a logo's `scale` is then a share of the photo's width);
    - `tile` is `width`, repeated over the whole photo with one tile at the anchor.
    `at` says where in the free space a picture smaller or larger than the area sits (the tiles' phase for `tile`).
    `margin` is a length in px, kept clear on every side (at most as much as leaves a pixel of the shorter side).
    `resample` is the filter that sizes it; colours are weighted by alpha while sizing, so a transparent pixel lends
    its neighbours no colour.
    """
    height, width = shape
    short = min(shape)
    inset = min(round(margin), (short - 1) // 2)
    area_h, area_w = height - 2 * inset, width - 2 * inset
    source_h, source_w = picture.shape[:2]
    if fit == "stretch":
        size_w, size_h = area_w, area_h
    else:
        factor = {
            "cover": max(area_w / source_w, area_h / source_h),
            "contain": min(area_w / source_w, area_h / source_h),
            "width": width / source_w,
            "tile": width / source_w,
        }[fit]
        size_w, size_h = source_w * factor, source_h * factor
    size_w, size_h = max(1, round(size_w * scale)), max(1, round(size_h * scale))
    sized = _resized(picture, (size_w, size_h), resample)

    # the share of the free space before the picture: 0 at the left or top, a half in the middle, 1 at the right or
    # bottom
    across = (("right" in at) - ("left" in at) + 1) / 2
    down = (("bottom" in at) - ("top" in at) + 1) / 2
    left = inset + round((area_w - size_w) * across)
    top = inset + round((area_h - size_h) * down)
    if fit == "tile":
        return sized[(np.arange(height) - top) % size_h][:, (np.arange(width) - left) % size_w]
    placed = np.zeros((height, width, sized.shape[2]), np.float32)
    first_row, first_column = max(top, 0), max(left, 0)
    last_row, last_column = min(top + size_h, height), min(left + size_w, width)
    if last_row > first_row and last_column > first_column:
        placed[first_row:last_row, first_column:last_column] = sized[
            first_row - top : last_row - top, first_column - left : last_column - left
        ]
    return placed


def _resized(picture: np.ndarray, size: tuple[int, int], resample: Resample) -> np.ndarray:
    """`picture` (RGBA, straight alpha) at `size` (width, height), sized with colours weighted by alpha."""
    alpha = picture[..., 3:]
    weighted = np.concatenate([picture[..., :3] * alpha, alpha], axis=2)
    # a float32 (height, width) array is Pillow's 32-bit float mode "F" by its dtype
    planes = [
        np.asarray(Image.fromarray(np.ascontiguousarray(weighted[..., c])).resize(size, _FILTERS[resample]))
        for c in range(weighted.shape[2])
    ]
    sized = np.clip(np.dstack(planes), 0.0, 1.0)
    sized_alpha = sized[..., 3:]
    colour = np.divide(sized[..., :3], sized_alpha, out=np.zeros_like(sized[..., :3]), where=sized_alpha > 0)
    return np.concatenate([np.clip(colour, 0.0, 1.0), sized_alpha], axis=2)
