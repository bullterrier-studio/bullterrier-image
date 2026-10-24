"""Warping a picture: reading it at float coordinates, and the coordinates of a warp about a centre."""

from dataclasses import dataclass
from typing import Literal

import numpy as np
from scipy import ndimage

# The one table of the edge rules for scipy's interpolating functions (`map_coordinates`, `affine_transform`), in their
# own spelling: for interpolation scipy's 'wrap' does not repeat the picture but 'grid-wrap' does. `photo_fx.ops.radial`
# reads it too, so the two modules cannot disagree about what an edge rule means.
SCIPY_EDGE = {"reflect": "reflect", "mirror": "mirror", "nearest": "nearest", "wrap": "grid-wrap"}
# The one table of the spline orders, read by `photo_fx.ops.radial` too.
# constant: scipy's spline order for each documented interpolation choice (0 nearest pixel, 1 linear, 3 cubic)
SPLINE_ORDER = {"nearest": 0, "bilinear": 1, "bicubic": 3}
# constant: columns copied from the far side when columns wrap; a cubic spline's end effect falls by a factor of
# 0.27 per pixel (2 - sqrt 3), so 16 leave under 1e-8
_SEAM = 16


def remap(
    rgb: np.ndarray,
    rows: np.ndarray,
    cols: np.ndarray,
    *,
    interpolation: Literal["nearest", "bilinear", "bicubic"],
    edge: Literal["reflect", "mirror", "nearest", "wrap"],
    wrap_columns: bool = False,
) -> np.ndarray:
    """The picture `rgb` read at the float positions (`rows`, `cols`), each colour channel on its own.

    `rows` and `cols` have the same shape, any shape, and say where to read: a pixel's centre is at whole numbers,
    row 0 and column 0 the first. The result has that shape plus the channels, float32, in 0..1 (a cubic spline can
    overshoot; it is clipped). `interpolation` is how a position between pixels is read: 'nearest' the closest pixel,
    'bilinear' the blend of the four around it, 'bicubic' a cubic spline through the neighbours. `edge` is what lies
    beyond the picture: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel),
    'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). Reading every pixel where it is returns the
    picture; `rgb` is never modified.

    With `wrap_columns` the picture is periodic across its columns, as an angle is: the column after the last is the
    first, whatever the `edge` rule, which then only applies to the rows (a position at any column, however far,
    is read modulo the width).
    """
    if wrap_columns:
        width = rgb.shape[1]
        rgb = rgb[:, np.arange(-_SEAM, width + _SEAM) % width]  # the picture with its far side copied on each side
        cols = cols % width + _SEAM
    where = np.stack([rows, cols])
    out = np.stack(
        [
            ndimage.map_coordinates(
                np.ascontiguousarray(rgb[..., channel]),
                where,
                order=SPLINE_ORDER[interpolation],
                mode=SCIPY_EDGE[edge],
            )
            for channel in range(rgb.shape[2])
        ],
        axis=-1,
    )
    return np.clip(out, 0, 1)


@dataclass(frozen=True)
class Centre:
    """A point in a picture of `height` x `width` px that a warp is about, with every pixel's place measured from it.

    `x` and `y` are fractions of the way from the first pixel to the last (0 the first column or row, 0.5 the
    middle, 1 the last), so the centre can sit on a pixel. `distance` is each pixel's distance from the centre in
    pixels, shape (height, width); the centre pixel's is 0.
    """

    height: int
    width: int
    x: float
    y: float

    @property
    def distance(self) -> np.ndarray:
        """The distance in px of every pixel from the centre, float64, shape (height, width)."""
        return np.hypot(*self.offsets())

    def sources(
        self, scale: float | np.ndarray | None = None, turn: float | np.ndarray | None = None
    ) -> tuple[np.ndarray, np.ndarray]:
        """Where each pixel of a radial warp reads the picture: its offset from the centre scaled, then turned.

        Returns (rows, cols), float64, each of shape (height, width), for `remap`. A pixel's source is the centre plus
        the pixel's offset from it multiplied by `scale` (2 reads twice as far out, 0.5 half as far) and then turned
        clockwise by `turn` radians (rows run down and columns right, so this is the way a clock's hands go on the
        picture). `scale` and `turn` are each a number, or an array of shape (height, width) that differs from pixel to
        pixel; None (the default) leaves that step out. The centre pixel always reads itself; with neither given
        every pixel reads itself.
        """
        dr, dc = self.offsets()
        if scale is not None:
            dr, dc = dr * scale, dc * scale
        if turn is not None:
            dr, dc = dr * np.cos(turn) - dc * np.sin(turn), dr * np.sin(turn) + dc * np.cos(turn)
        centre_row, centre_col = self.y * (self.height - 1), self.x * (self.width - 1)
        return np.broadcast_to(centre_row + dr, self.shape), np.broadcast_to(centre_col + dc, self.shape)

    @property
    def shape(self) -> tuple[int, int]:
        """The shape of the picture, (height, width)."""
        return self.height, self.width

    def offsets(self) -> tuple[np.ndarray, np.ndarray]:
        """Where each pixel is from the centre, in px: the row offset, shape (height, 1), and the column offset, shape
        (1, width), both float64, positive below and right of the centre. They broadcast against each other, and against
        `distance`, to (height, width)."""
        dr = np.arange(self.height, dtype=np.float64)[:, None] - self.y * (self.height - 1)
        dc = np.arange(self.width, dtype=np.float64)[None, :] - self.x * (self.width - 1)
        return dr, dc
