"""The pixel geometry of framing a photo: flip, rotate, the crop box, pad, and the alpha of a circle or rounded corners.

Every function takes pixels (the caller turns shares of the picture into pixels) and works on float32 pictures of any
number of channels, so a subject matte can travel as a fourth channel and move with the photo. Rotation is
counter-clockwise for a positive angle, as on a clock seen in a mirror: it is the direction `numpy.rot90` turns.
"""

import math
from collections.abc import Sequence
from typing import Literal, NamedTuple

import numpy as np
from PIL import Image

from bullterrier_image.errors import SubjectNotFoundError

Side = Literal["center", "top", "bottom", "left", "right"]
Fit = Literal["crop", "expand"]
Resample = Literal["nearest", "bilinear", "bicubic"]

_FILTERS = {
    "nearest": Image.Resampling.NEAREST,
    "bilinear": Image.Resampling.BILINEAR,
    "bicubic": Image.Resampling.BICUBIC,
}
_QUARTER = 90  # constant: degrees in a quarter turn
_DIGITS = 6  # constant: sizes are rounded to a millionth of a pixel before they are cut to whole pixels, so that the
# 1e-16 of cos(90 degrees) cannot move a size by one pixel
_HALF_PIXEL = 0.5  # constant: a pixel's centre is half a pixel from its edge (the soft edge of a cut is one pixel wide)


class Box(NamedTuple):
    """A rectangle of a picture in pixels."""

    top: int
    left: int
    height: int
    width: int

    def of(self, array: np.ndarray) -> np.ndarray:
        """The part of `array` (rows, columns, ...) this box covers, as a view."""
        return array[self.top : self.top + self.height, self.left : self.left + self.width]


def flip(array: np.ndarray, *, x: bool, y: bool) -> np.ndarray:
    """`array` mirrored left to right (`x`) and/or top to bottom (`y`), as a view."""
    return array[:: -1 if y else 1, :: -1 if x else 1]


def rotate(array: np.ndarray, angle: float, *, fit: Fit, fill: Sequence[float], resample: Resample) -> np.ndarray:
    """`array` (rows, columns, channels) turned by `angle` degrees about its centre, counter-clockwise when positive.

    `fit="expand"` makes the picture as large as the whole turned picture needs, `fill` (one value per channel) showing
    in the corners; `fit="crop"` keeps the largest rectangle of the original's aspect that the turned picture covers
    everywhere, so no fill shows (`fill` is then unused). A quarter turn moves every pixel exactly; any other angle is
    resampled with `resample` (the colour at an edge of the turned picture is blended into `fill` over one pixel).
    """
    height, width = array.shape[:2]
    cos, sin = (abs(f(math.radians(angle))) for f in (math.cos, math.sin))
    if fit == "expand":
        size = (_up(width * cos + height * sin), _up(width * sin + height * cos))
    else:
        scale = min(1 / (cos + height / width * sin), 1 / (width / height * sin + cos))
        size = (max(1, _down(width * scale)), max(1, _down(height * scale)))
    if angle % _QUARTER == 0:
        turned = np.rot90(array, round(angle / _QUARTER))
        top, left = (turned.shape[0] - size[1]) // 2, (turned.shape[1] - size[0]) // 2
        return turned if fit == "expand" else np.ascontiguousarray(Box(top, left, size[1], size[0]).of(turned))

    # PIL maps each output pixel's position to the input's: x_in = a x + b y + c, y_in = d x + e y + f, positions in
    # pixel-corner coordinates, so the middles are half the sizes
    radians = math.radians(angle)
    a, b, d, e = math.cos(radians), -math.sin(radians), math.sin(radians), math.cos(radians)
    centre = (size[0] / 2, size[1] / 2)
    matrix = (a, b, width / 2 - a * centre[0] - b * centre[1], d, e, height / 2 - d * centre[0] - e * centre[1])
    turned = np.empty((size[1], size[0], array.shape[2]), np.float32)
    for channel in range(array.shape[2]):
        image = Image.fromarray(np.ascontiguousarray(array[..., channel], np.float32))
        moved = image.transform(size, Image.Transform.AFFINE, matrix, _FILTERS[resample])
        turned[..., channel] = np.asarray(moved)
    if fit == "expand":
        # how far inside the picture's edge each output pixel's centre lies, in pixels, gives a one-pixel soft edge
        columns, rows = np.meshgrid(
            np.arange(size[0]) + _HALF_PIXEL - centre[0], np.arange(size[1]) + _HALF_PIXEL - centre[1]
        )
        x_in = a * columns + b * rows + width / 2
        y_in = d * columns + e * rows + height / 2
        inside = np.minimum(np.minimum(x_in, width - x_in), np.minimum(y_in, height - y_in))
        covered = np.clip(inside + _HALF_PIXEL, 0.0, 1.0).astype(np.float32)[..., None]
        turned = turned * covered + np.asarray(fill, np.float32) * (1.0 - covered)
    return np.clip(turned, 0.0, 1.0, dtype=np.float32)


def _up(pixels: float) -> int:
    return math.ceil(round(pixels, _DIGITS))


def _down(pixels: float) -> int:
    return math.floor(round(pixels, _DIGITS))


def subject_box(matte: np.ndarray, margin: float) -> Box:
    """The box around the subject (the matte's pixels at one half or more), with `margin` (a share of the box's longer
    side) added on every side and kept inside the picture. SubjectNotFoundError when no pixel reaches one half."""
    rows, columns = np.nonzero(matte >= _HALF_PIXEL)  # constant: a matte value of one half is the subject's edge
    if rows.size == 0:
        raise SubjectNotFoundError("the subject matte has no pixel at one half or more: there is no subject to frame")
    top, bottom, left, right = (int(edge) for edge in (rows.min(), rows.max() + 1, columns.min(), columns.max() + 1))
    extra = round(margin * max(bottom - top, right - left))
    top, left = max(top - extra, 0), max(left - extra, 0)
    bottom, right = min(bottom + extra, matte.shape[0]), min(right + extra, matte.shape[1])
    return Box(top, left, bottom - top, right - left)


def crop_box(shape: tuple[int, int], aspect: float, anchor: Side) -> Box:
    """The largest part of a picture of `shape` (rows, columns) whose width over its height is `aspect`, put in the
    middle of the picture or against its `anchor` side (a rectangle that fills the picture in one direction has no room
    to move that way)."""
    height, width = shape
    crop_width, crop_height = _largest(aspect, width, height)
    # the share of the free room before the crop: 0 at the start, a half in the middle, 1 at the end
    across = ((anchor == "right") - (anchor == "left") + 1) / 2
    down = ((anchor == "bottom") - (anchor == "top") + 1) / 2
    return Box(round((height - crop_height) * down), round((width - crop_width) * across), crop_height, crop_width)


def crop_around(shape: tuple[int, int], subject: Box, aspect: float | None) -> Box:
    """The smallest part of a picture of `shape` that holds the `subject` box and has width over height `aspect`,
    centred on the subject and moved inside the picture. When the picture is too small to hold the subject at that
    aspect, the largest such part that fits, centred on it. Without an `aspect`, the subject's box itself."""
    height, width = shape
    if aspect is None:
        return subject
    if subject.width < aspect * subject.height:
        crop_width, crop_height = math.ceil(subject.height * aspect), subject.height
    else:
        crop_width, crop_height = subject.width, math.ceil(subject.width / aspect)
    if crop_width > width or crop_height > height:
        crop_width, crop_height = _largest(aspect, width, height)
    top = round(subject.top + (subject.height - crop_height) / 2)
    left = round(subject.left + (subject.width - crop_width) / 2)
    return Box(min(max(top, 0), height - crop_height), min(max(left, 0), width - crop_width), crop_height, crop_width)


def _largest(aspect: float, width: int, height: int) -> tuple[int, int]:
    """The width and height of the largest rectangle of `aspect` (width over height) inside `width` by `height`."""
    if width > height * aspect:
        return max(1, round(height * aspect)), height
    return width, max(1, round(width / aspect))


def pad(array: np.ndarray, *, top: int, right: int, bottom: int, left: int, fill: Sequence[float]) -> np.ndarray:
    """`array` on a larger canvas: `top`, `right`, `bottom` and `left` pixels added, each channel filled with its value
    in `fill`."""
    height, width, channels = array.shape
    padded = np.empty((height + top + bottom, width + left + right, channels), np.float32)
    padded[:] = np.asarray(fill, np.float32)
    padded[top : top + height, left : left + width] = array
    return padded


def circle_alpha(shape: tuple[int, int]) -> np.ndarray:
    """Alpha (rows, columns) of the circle inscribed in a picture of `shape`: 1 inside, 0 outside, a soft edge one pixel
    wide."""
    height, width = shape
    radius = min(shape) / 2
    rows = np.arange(height, dtype=np.float32) + _HALF_PIXEL - height / 2
    columns = np.arange(width, dtype=np.float32) + _HALF_PIXEL - width / 2
    distance = np.hypot(rows[:, None], columns[None, :])
    return np.clip(radius - distance + _HALF_PIXEL, 0.0, 1.0).astype(np.float32)


def rounded_alpha(shape: tuple[int, int], radius: float) -> np.ndarray:
    """Alpha (rows, columns) of a picture of `shape` with its corners rounded by `radius` pixels: 1 inside, 0 outside, a
    one-pixel soft edge."""
    height, width = shape
    rows = np.abs(np.arange(height, dtype=np.float32) + _HALF_PIXEL - height / 2) - (height / 2 - radius)
    columns = np.abs(np.arange(width, dtype=np.float32) + _HALF_PIXEL - width / 2) - (width / 2 - radius)
    # the signed distance to a rounded rectangle (negative inside): the distance to the corner's centre beyond the arc's
    # start, or how far inside the straight sides
    outside = np.hypot(np.maximum(rows[:, None], 0.0), np.maximum(columns[None, :], 0.0))
    inside = np.minimum(np.maximum(rows[:, None], columns[None, :]), 0.0)
    return np.clip(_HALF_PIXEL - (outside + inside - radius), 0.0, 1.0).astype(np.float32)
